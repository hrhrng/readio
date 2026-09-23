ALTER TABLE tts_jobs
ADD COLUMN operation_id TEXT;

UPDATE tts_jobs
SET operation_id = 'legacy-' || id
WHERE operation_id IS NULL;

CREATE INDEX tts_jobs_operation_idx
ON tts_jobs(user_id, operation_id);
