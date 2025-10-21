-- Create tables for SabreDAV
CREATE TABLE IF NOT EXISTS `users` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  username VARCHAR(50) NOT NULL,
  digesta1 TEXT NOT NULL,
  UNIQUE(username)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `principals` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  uri VARCHAR(255) NOT NULL,
  email VARCHAR(80),
  displayname VARCHAR(80),
  UNIQUE(uri)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `groups` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  uri VARCHAR(255) NOT NULL,
  displayname VARCHAR(80),
  UNIQUE(uri)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `groupmembers` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  principal_id INTEGER UNSIGNED NOT NULL,
  member_id INTEGER UNSIGNED NOT NULL,
  UNIQUE(principal_id, member_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `calendarobjects` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  calendardata MEDIUMBLOB,
  uri VARCHAR(255),
  calendarid INTEGER UNSIGNED NOT NULL,
  lastmodified INT(11),
  etag VARCHAR(32),
  size INT(11) UNSIGNED NOT NULL,
  componenttype VARCHAR(8),
  firstoccurence INT(11),
  lastoccurence INT(11),
  uid VARCHAR(255),
  UNIQUE(calendarid, uri)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `calendars` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  principaluri VARCHAR(255),
  displayname VARCHAR(100),
  uri VARCHAR(255),
  synctoken INTEGER UNSIGNED NOT NULL DEFAULT '1',
  description TEXT,
  calendarorder INTEGER UNSIGNED NOT NULL DEFAULT '0',
  calendarcolor VARCHAR(10),
  timezone TEXT,
  components VARCHAR(50),
  transparent TINYINT(1) NOT NULL DEFAULT '0',
  UNIQUE(principaluri, uri)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `calendarchanges` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  calendarid INTEGER UNSIGNED NOT NULL,
  uri VARCHAR(255),
  synctoken INTEGER UNSIGNED NOT NULL,
  calendarobjectid INTEGER UNSIGNED,
  operation TINYINT(1) NOT NULL,
  INDEX calendarid_synctoken (calendarid, synctoken)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Insert test user (username: test, password: test - plain text for development)
INSERT INTO `users` (`username`, `digesta1`) VALUES
('test', 'test');

-- Insert principal for test user
INSERT INTO `principals` (`uri`, `email`, `displayname`) VALUES
('principals/test', 'test@example.com', 'Test User');

-- Create a default calendar for the test user
INSERT INTO `calendars` (`principaluri`, `displayname`, `uri`, `description`, `components`) VALUES
('principals/test', 'Default Calendar', 'default', 'Default calendar', 'VEVENT,VTODO');