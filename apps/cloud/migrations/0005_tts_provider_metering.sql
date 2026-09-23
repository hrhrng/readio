ALTER TABLE tts_jobs
ADD COLUMN requested_model_id TEXT NOT NULL DEFAULT 'readio/auto';

ALTER TABLE tts_jobs
ADD COLUMN language TEXT NOT NULL DEFAULT 'und';

ALTER TABLE tts_jobs
ADD COLUMN input_characters INTEGER NOT NULL DEFAULT 0
CHECK (input_characters >= 0);

ALTER TABLE tts_jobs
ADD COLUMN provider TEXT;

ALTER TABLE tts_jobs
ADD COLUMN resolved_model_id TEXT;
