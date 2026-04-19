-- Prevent duplicate participant rows if POST /games/result is called more than once.
CREATE UNIQUE INDEX IF NOT EXISTS idx_participants_unique
    ON game_participants(game_record_id, player_id);
