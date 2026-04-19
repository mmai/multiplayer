[working-directory: 'deploy']
run-relay:
  ./relay-server

[working-directory: 'games/leptos-tic-tac-toe']
dev-leptos:
  trunk serve

[working-directory: 'games/leptos-tic-tac-toe']
build-leptos:
  trunk build --release
  cp dist/index.html ../../deploy/tic-tac-toe.html
  cp dist/*.wasm ../../deploy/
  cp dist/*.js ../../deploy/
  cp dist/*.css ../../deploy/

[working-directory: 'games/leptos-user-portal']
dev-portal:
  trunk serve

[working-directory: 'games/leptos-user-portal']
build-portal:
  trunk build --release
  cp dist/index.html ../../deploy/portal.html
  cp dist/*.wasm ../../deploy/
  cp dist/*.js ../../deploy/
  cp dist/*.css ../../deploy/

build-relay:
  CARGO_PROFILE_RELEASE_OPT_LEVEL=3 cargo build -p relay-server --release
  mkdir -p deploy
  cp target/release/relay-server deploy
