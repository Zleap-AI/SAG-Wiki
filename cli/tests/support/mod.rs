use std::{path::{Path,PathBuf},process::Command};
pub struct ServiceCleanup(pub PathBuf);
impl Drop for ServiceCleanup {
    fn drop(&mut self) {
        if self.0.join("sag-service.json").exists() {
            let output=Command::new(env!("CARGO_BIN_EXE_sag")).env("SAG_DATA_DIR",&self.0).args(["service","stop"]).output().unwrap();
            if !output.status.success(){eprintln!("service cleanup: {}",String::from_utf8_lossy(&output.stderr));}
        }
    }
}
pub fn serve(data:&Path,count:usize,mut handler:impl FnMut(serde_json::Value)->serde_json::Value+Send+'static)->std::thread::JoinHandle<()> {
    // The mock server lives in this test process; PID 1 is not portable to Windows.
    let pid = std::process::id();
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
    std::fs::write(data.join("sag-service.json"),serde_json::to_vec(&sag_protocol::Endpoint{protocol_version:sag_protocol::VERSION,port:listener.local_addr().unwrap().port(),pid,token:"test-token".into()}).unwrap()).unwrap();
    std::thread::spawn(move||tokio::runtime::Runtime::new().unwrap().block_on(async move{
        use sag_protocol::*;
        let listener=tokio::net::TcpListener::from_std(listener).unwrap();
        for _ in 0..count {
            let(stream,_)=tokio::time::timeout(std::time::Duration::from_secs(10),listener.accept()).await.unwrap().unwrap();
            let(read,mut write)=stream.into_split();let mut read=tokio::io::BufReader::new(read);
            assert!(matches!(sag_client::wire::read(&mut read).await.unwrap(),Some(Frame::Hello{..})));
            sag_client::wire::write(&mut write,&Frame::Welcome{info:ServiceInfo{version:Some(BUILD_VERSION.into()),protocol_version:VERSION,build_id:BUILD_ID.into(),channel:BUILD_CHANNEL.into(),pid,executable:"test-sag".into(),data_directory:"test-data".into()}}).await.unwrap();
            let Some(Frame::Request{id,operation:Operation::Cli{path,command}})=sag_client::wire::read(&mut read).await.unwrap()else{panic!("CLI request expected")};
            let reply:Response=serde_json::from_value(handler(serde_json::json!({"path":path,"command":command}))).unwrap();
            sag_client::wire::write(&mut write,&Frame::Response{id,result:Ok(Output::Cli(reply))}).await.unwrap();
        }
    }))
}
