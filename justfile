[working-directory: 'games/dioxus-tic-tac-toe']
dev-dioxus:
  dx serve --platform web --port 9090

[working-directory: 'games/dioxus-tic-tac-toe']
build-dioxus:
  dx build --platform web --release # production build → dist/
