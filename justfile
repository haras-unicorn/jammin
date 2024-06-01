set windows-shell := ["nu.exe", "-c"]
set shell := ["nu", "-c"]

root := absolute_path('')

format:
  cd '{{root}}'; cargo fmt
  prettier --write '{{root}}'
  nixpkgs-fmt '{{root}}'

lint:
  cd '{{root}}'; cargo clippy

build:
  cd '{{root}}'; cargo build --release

test:
  cd '{{root}}'; cargo test

docs:
  cd '{{root}}'; cargo doc --no-deps

run *args:
  cd '{{root}}'; cargo run -- {{args}}
