#!/bin/zsh
set -e
app_dir="${0:A:h}/src-tauri/target/release/bundle/macos/SAG Desktop.app"
if [[ ! -d "$app_dir" ]]; then
  print "尚未生成客户端。请在项目目录执行 npm run tauri build -- --bundles app。"
  exit 1
fi
/usr/bin/open "$app_dir"
