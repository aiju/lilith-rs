set -e

(cd host && cargo build)
(cd cmd && cargo build)
./host/target/debug/fs-tool fs.img ./cmd/target/x86_64-lilith/debug/cat
(cd kernel && cargo run)