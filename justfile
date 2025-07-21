build-sync DEST PASS:
  cross build
  sshpass -p "{{PASS}}" rsync target/aarch64-unknown-linux-gnu/debug/cnc-control {{DEST}}:~/bin/

