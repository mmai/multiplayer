build-relay:
  CARGO_PROFILE_RELEASE_OPT_LEVEL=3 cargo build -p relay-server --release
  mkdir -p deploy
  cp target/release/relay-server deploy

[working-directory: 'games/dioxus-tic-tac-toe']
dev-dioxus:
  dx serve --platform web --port 9090

[working-directory: 'games/dioxus-tic-tac-toe']
build-dioxus:
  dx bundle --web --release
