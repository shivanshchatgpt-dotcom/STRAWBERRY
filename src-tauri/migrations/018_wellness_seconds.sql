-- Wellness intervals: switch from coarse `interval_minutes` (integer minutes only)
-- to a flexible `interval_seconds` (i64 seconds). Old rows are migrated by
-- multiplying by 60, so 10 minutes → 600 seconds, 45 minutes → 2700 seconds, etc.
-- This lets the UI expose a seconds / minutes / hours unit picker.

ALTER TABLE wellness_config ADD COLUMN interval_seconds INTEGER;

UPDATE wellness_config
   SET interval_seconds = interval_minutes * 60
 WHERE interval_seconds IS NULL;

ALTER TABLE wellness_config DROP COLUMN interval_minutes;
