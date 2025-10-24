-- Insert sample calendar events for testing
-- This will create a large collection of events for E2E testing

INSERT INTO `calendarobjects` (`calendardata`, `uri`, `calendarid`, `lastmodified`, `etag`, `size`, `componenttype`, `firstoccurence`, `lastoccurence`, `uid`) 
VALUES 
('BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//SabreDAV//SabreDAV Server//EN
BEGIN:VEVENT
UID:event1@example.com
DTSTART:20230101T100000Z
DTEND:20230101T110000Z
SUMMARY:Test Event 1
DESCRIPTION:This is a test event for E2E testing
END:VEVENT
END:VCALENDAR', 'event1.ics', 1, UNIX_TIMESTAMP(), 'abc123', 200, 'VEVENT', 1672562400, 1672566000, 'event1@example.com'),

('BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//SabreDAV//SabreDAV Server//EN
BEGIN:VEVENT
UID:event2@example.com
DTSTART:20230102T100000Z
DTEND:20230102T110000Z
SUMMARY:Test Event 2
DESCRIPTION:This is a test event for E2E testing
END:VEVENT
END:VCALENDAR', 'event2.ics', 1, UNIX_TIMESTAMP(), 'def456', 200, 'VEVENT', 1672648800, 1672652400, 'event2@example.com'),

('BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//SabreDAV//SabreDAV Server//EN
BEGIN:VEVENT
UID:event3@example.com
DTSTART:20230103T100000Z
DTEND:20230103T110000Z
SUMMARY:Test Event 3
DESCRIPTION:This is a test event for E2E testing
END:VEVENT
END:VCALENDAR', 'event3.ics', 1, UNIX_TIMESTAMP(), 'ghi789', 200, 'VEVENT', 1672735200, 1672738800, 'event3@example.com');