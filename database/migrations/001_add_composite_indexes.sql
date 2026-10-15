-- EventPro Database Migration: 001_add_composite_indexes.sql
-- Description: Adds non-destructive composite indexes for booking history and organizer event listings.
-- Safe and idempotent: Checks information_schema before creating indexes.

DELIMITER $$

DROP PROCEDURE IF EXISTS AddCompositeIndexes$$
CREATE PROCEDURE AddCompositeIndexes()
BEGIN
    -- 1. Composite index on bookings(user_id, booked_at DESC)
    -- Optimizes: SELECT ... FROM bookings WHERE user_id = ? ORDER BY booked_at DESC
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.statistics 
        WHERE table_schema = DATABASE() 
          AND table_name = 'bookings' 
          AND index_name = 'idx_bookings_user_booked'
    ) THEN
        CREATE INDEX idx_bookings_user_booked ON bookings(user_id, booked_at DESC);
        SELECT 'Index idx_bookings_user_booked created successfully.' AS status;
    ELSE
        SELECT 'Index idx_bookings_user_booked already exists. Skipping.' AS status;
    END IF;

    -- 2. Composite index on events(organizer_id, created_at DESC)
    -- Optimizes: SELECT ... FROM events WHERE organizer_id = ? ORDER BY created_at DESC
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.statistics 
        WHERE table_schema = DATABASE() 
          AND table_name = 'events' 
          AND index_name = 'idx_events_organizer_created'
    ) THEN
        CREATE INDEX idx_events_organizer_created ON events(organizer_id, created_at DESC);
        SELECT 'Index idx_events_organizer_created created successfully.' AS status;
    ELSE
        SELECT 'Index idx_events_organizer_created already exists. Skipping.' AS status;
    END IF;
END$$

DELIMITER ;

CALL AddCompositeIndexes();
DROP PROCEDURE IF EXISTS AddCompositeIndexes;
