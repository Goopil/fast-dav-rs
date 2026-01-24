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

-- Insert sample addressbook cards for testing
INSERT INTO `cards` (`carddata`, `uri`, `addressbookid`, `lastmodified`, `etag`, `size`)
VALUES
('BEGIN:VCARD
VERSION:4.0
UID:contact1@example.com
FN:Test Contact 1
EMAIL:contact1@example.com
END:VCARD', 'contact1.vcf', 1, UNIX_TIMESTAMP(), 'card123', 150),

('BEGIN:VCARD
VERSION:4.0
UID:contact2@example.com
FN:Test Contact 2
EMAIL:contact2@example.com
END:VCARD', 'contact2.vcf', 1, UNIX_TIMESTAMP(), 'card456', 150);
