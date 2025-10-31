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

CREATE TABLE IF NOT EXISTS `calendarinstances` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  calendarid INTEGER UNSIGNED NOT NULL,
  principaluri VARCHAR(255),
  access TINYINT(1) NOT NULL DEFAULT '1',
  displayname VARCHAR(100),
  uri VARCHAR(255),
  description TEXT,
  calendarorder INTEGER UNSIGNED NOT NULL DEFAULT '0',
  calendarcolor VARCHAR(10),
  timezone TEXT,
  transparent TINYINT(1) NOT NULL DEFAULT '0',
  share_href VARCHAR(255),
  share_displayname VARCHAR(100),
  share_invitestatus TINYINT(1) NOT NULL DEFAULT '2',
  UNIQUE(principaluri, uri),
  UNIQUE(calendarid, principaluri)
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
  synctoken INTEGER UNSIGNED NOT NULL DEFAULT '1',
  components VARCHAR(50)
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

CREATE TABLE IF NOT EXISTS `calendarsubscriptions` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  uri VARCHAR(255),
  principaluri VARCHAR(255),
  source TEXT,
  displayname VARCHAR(100),
  refreshrate VARCHAR(10),
  calendarorder INTEGER UNSIGNED NOT NULL DEFAULT '0',
  calendarcolor VARCHAR(10),
  striptodos TINYINT(1) NULL,
  stripalarms TINYINT(1) NULL,
  stripattachments TINYINT(1) NULL,
  lastmodified INT(11)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `schedulingobjects` (
  id INTEGER UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
  principaluri VARCHAR(255),
  calendardata MEDIUMBLOB,
  uri VARCHAR(255),
  lastmodified INT(11),
  etag VARCHAR(32),
  size INT(11) UNSIGNED NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Insert test user (username: test, password: test - plain text for development)
INSERT INTO `users` (`username`, `digesta1`) VALUES
('test', 'test');

-- Insert principal for test user
INSERT INTO `principals` (`uri`, `email`, `displayname`) VALUES
('principals/test', 'test@example.com', 'Test User');

-- Create a default calendar for the test user
INSERT INTO `calendars` (`components`) VALUES
('VEVENT,VTODO');

INSERT INTO `calendarinstances` (`calendarid`, `principaluri`, `displayname`, `uri`, `description`) VALUES
(1, 'principals/test', 'Default Calendar', 'default', 'Default calendar');