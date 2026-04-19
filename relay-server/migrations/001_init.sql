CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT    NOT NULL UNIQUE,
    email         TEXT    NOT NULL UNIQUE,
    password_hash TEXT    NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS game_records (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id    TEXT    NOT NULL,
    room_code  TEXT    NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at   INTEGER,
    result     TEXT
);

CREATE TABLE IF NOT EXISTS game_participants (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    game_record_id INTEGER NOT NULL REFERENCES game_records(id),
    user_id        INTEGER REFERENCES users(id),
    player_id      INTEGER NOT NULL,
    outcome        TEXT
);
