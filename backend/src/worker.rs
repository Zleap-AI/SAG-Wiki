#[cfg(target_os = "linux")]
mod linux {
    use anyhow::{bail, Context, Result};
    use sag_protocol::{McpProcessReply, McpProcessRequest};
    use std::{
        ffi::{CStr, CString},
        fs::{self, OpenOptions},
        os::unix::{
            fs::{OpenOptionsExt, PermissionsExt},
            process::CommandExt,
        },
        path::PathBuf,
        process::Stdio,
    };
    use tokio::{
        io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
        net::{UnixListener, UnixStream},
    };
    use tokio_util::sync::CancellationToken;

    struct User {
        name: String,
        gid: u32,
        groups: Vec<u32>,
    }

    fn account(uid: u32, app_uid: u32) -> Result<User> {
        if uid == 0 || uid == app_uid {
            bail!("本地 MCP 必须使用飞牛登录用户，禁止 root 和应用账户");
        }
        let mut record = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        let mut buffer = vec![0u8; 16384];
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                record.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status != 0 {
            return Err(std::io::Error::from_raw_os_error(status).into());
        }
        if result.is_null() {
            bail!("飞牛用户不存在");
        }
        let record = unsafe { record.assume_init() };
        let name = unsafe { CStr::from_ptr(record.pw_name) }
            .to_str()?
            .to_owned();
        let c_name = CString::new(name.as_str())?;
        let mut count = 0;
        unsafe {
            libc::getgrouplist(
                c_name.as_ptr(),
                record.pw_gid,
                std::ptr::null_mut(),
                &mut count,
            );
        }
        let mut groups = vec![0; usize::try_from(count)?];
        if unsafe {
            libc::getgrouplist(
                c_name.as_ptr(),
                record.pw_gid,
                groups.as_mut_ptr(),
                &mut count,
            )
        } < 0
        {
            bail!("无法读取飞牛用户的用户组");
        }
        groups.truncate(count as usize);
        Ok(User {
            name,
            gid: record.pw_gid,
            groups,
        })
    }

    async fn reply(stream: &mut UnixStream, value: McpProcessReply) -> Result<()> {
        stream.write_all(&serde_json::to_vec(&value)?).await?;
        stream.write_all(b"\n").await?;
        Ok(())
    }

    async fn serve(
        stream: UnixStream,
        app_uid: u32,
        state: PathBuf,
        stop: CancellationToken,
    ) -> Result<()> {
        // The kernel supplies this identity; a JSON field cannot impersonate the app service.
        if stream.peer_cred()?.uid() != app_uid {
            bail!("拒绝非应用账户的 MCP 进程请求");
        }
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        {
            let mut limited = (&mut reader).take(65537);
            tokio::select! {
                _ = stop.cancelled() => return Ok(()),
                read = tokio::time::timeout(std::time::Duration::from_secs(10), limited.read_line(&mut line)) => { read.context("读取进程请求超时")??; }
            }
        }
        if line.len() > 65536 || !line.ends_with('\n') {
            bail!("本地命令请求超过大小限制");
        }
        let request: McpProcessRequest = serde_json::from_str(&line).context("本地命令请求无效")?;
        let user = match account(request.uid, app_uid) {
            Ok(user) => user,
            Err(error) => {
                reply(
                    reader.get_mut(),
                    McpProcessReply {
                        pid: None,
                        stderr_path: None,
                        error: Some(error.to_string()),
                    },
                )
                .await?;
                return Ok(());
            }
        };
        let directory = state.join(request.uid.to_string());
        fs::create_dir_all(&directory)?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o711))?;
        let home = directory.join("home");
        if !home.exists() {
            fs::create_dir(&home)?;
            let path = CString::new(home.as_os_str().as_encoded_bytes())?;
            if unsafe { libc::chown(path.as_ptr(), request.uid, user.gid) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
        }
        // Log directories are root-owned: callers cannot replace another user's log with a symlink.
        let logs = directory.join("logs");
        fs::create_dir_all(&logs)?;
        fs::set_permissions(&logs, fs::Permissions::from_mode(0o711))?;
        let stderr_path = logs.join(format!("{}.log", uuid::Uuid::new_v4()));
        let stderr = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&stderr_path)?;
        let log_path = CString::new(stderr_path.as_os_str().as_encoded_bytes())?;
        if unsafe { libc::chown(log_path.as_ptr(), request.uid, user.gid) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let cwd = CString::new(home.as_os_str().as_encoded_bytes())?;
        let uid = request.uid;
        let mut command = std::process::Command::new(&request.command);
        command
            .args(&request.args)
            .env_clear()
            .env(
                "PATH",
                "/var/apps/nodejs_v22/target/bin:/usr/local/bin:/usr/bin:/bin",
            )
            .env("HOME", &home)
            .env("USER", &user.name)
            .env("LOGNAME", &user.name)
            .env("LANG", "C.UTF-8")
            .envs(request.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr);
        unsafe {
            command.pre_exec(move || {
                if libc::setgroups(user.groups.len(), user.groups.as_ptr()) != 0
                    || libc::setgid(user.gid) != 0
                    || libc::setuid(uid) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                    || libc::setsid() < 0
                    || libc::chdir(cwd.as_ptr()) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                libc::umask(0o077);
                Ok(())
            });
        }
        let mut command = tokio::process::Command::from(command);
        command.kill_on_drop(true);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                reply(
                    reader.get_mut(),
                    McpProcessReply {
                        pid: None,
                        stderr_path: None,
                        error: Some(format!("无法以当前用户启动 MCP：{error}")),
                    },
                )
                .await?;
                return Ok(());
            }
        };
        let pid = child.id().context("无法读取 MCP 进程编号")?;
        let result: Result<()> = async {
            reply(reader.get_mut(), McpProcessReply { pid: Some(pid), stderr_path: Some(stderr_path.to_string_lossy().into_owned()), error: None }).await?;
            let mut input = child.stdin.take().unwrap();
            let mut output = child.stdout.take().unwrap();
            let (mut read, mut write) = tokio::io::split(reader);
            tokio::select! {
                _ = stop.cancelled() => Ok(()),
                result = tokio::io::copy(&mut read, &mut input) => result.map(|_| ()).map_err(Into::into),
                result = tokio::io::copy(&mut output, &mut write) => result.map(|_| ()).map_err(Into::into),
            }
        }.await;
        // Close the entire process group, including any subprocesses created by npx or Python.
        let killed = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        if killed != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(std::io::Error::last_os_error().into());
        }
        child.wait().await?;
        result
    }

    pub async fn run() -> Result<()> {
        if unsafe { libc::geteuid() } != 0 {
            bail!("MCP 进程服务需要由飞牛启动脚本初始化");
        }
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.len() != 3 {
            bail!("usage: sag-fnos-worker SOCKET APP_UID STATE_DIRECTORY");
        }
        let socket = PathBuf::from(&args[0]);
        let app_uid: u32 = args[1].to_str().context("应用 UID 无效")?.parse()?;
        if app_uid == 0 {
            bail!("应用服务必须使用非 root 账户");
        }
        let state = PathBuf::from(&args[2]);
        fs::create_dir_all(&state)?;
        fs::set_permissions(&state, fs::Permissions::from_mode(0o711))?;
        let listener = UnixListener::bind(&socket)?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        let socket_c = CString::new(socket.as_os_str().as_encoded_bytes())?;
        if unsafe { libc::chown(socket_c.as_ptr(), app_uid, !0) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let stop = CancellationToken::new();
        let mut tasks = tokio::task::JoinSet::new();
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        loop {
            tokio::select! {
                _ = terminate.recv() => break,
                _ = tokio::signal::ctrl_c() => break,
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    let state = state.clone(); let stop = stop.clone();
                    tasks.spawn(async move { serve(stream, app_uid, state, stop).await });
                }
                Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                    if let Err(error) = result? { eprintln!("本地 MCP 连接结束：{error}"); }
                }
            }
        }
        stop.cancel();
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result? {
                eprintln!("本地 MCP 停止：{error}");
            }
        }
        fs::remove_file(socket)?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::run().await
    }
    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!("飞牛进程服务仅在 Linux 上运行")
    }
}
