-- ============================================================
-- End-to-End Event Lifecycle & Ticketing Platform
-- Database Schema
-- ============================================================

CREATE DATABASE IF NOT EXISTS event_lifecycle
  CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

USE event_lifecycle;

-- -------------------------------------------------------
-- USERS
-- -------------------------------------------------------
CREATE TABLE IF NOT EXISTS users (
  id          INT AUTO_INCREMENT PRIMARY KEY,
  full_name   VARCHAR(100) NOT NULL,
  email       VARCHAR(150) NOT NULL UNIQUE,
  password    VARCHAR(255) NOT NULL,
  role        ENUM('user','organizer','admin') NOT NULL DEFAULT 'user',
  avatar      VARCHAR(255) DEFAULT NULL,
  bio         TEXT DEFAULT NULL,
  created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

-- -------------------------------------------------------
-- CATEGORIES
-- -------------------------------------------------------
CREATE TABLE IF NOT EXISTS categories (
  id    INT AUTO_INCREMENT PRIMARY KEY,
  name  VARCHAR(80) NOT NULL UNIQUE,
  icon  VARCHAR(50) DEFAULT '🎫'
);

-- -------------------------------------------------------
-- EVENTS
-- -------------------------------------------------------
CREATE TABLE IF NOT EXISTS events (
  id              INT AUTO_INCREMENT PRIMARY KEY,
  organizer_id    INT NOT NULL,
  category_id     INT DEFAULT NULL,
  title           VARCHAR(200) NOT NULL,
  description     TEXT,
  venue           VARCHAR(255),
  city            VARCHAR(100),
  event_date      DATETIME NOT NULL,
  registration_deadline DATETIME,
  total_seats     INT NOT NULL DEFAULT 100,
  available_seats INT NOT NULL DEFAULT 100,
  ticket_price    DECIMAL(10,2) NOT NULL DEFAULT 0.00,
  banner_image    VARCHAR(255) DEFAULT NULL,
  status          ENUM('draft','published','cancelled','completed') DEFAULT 'draft',
  created_at      DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at      DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  FOREIGN KEY (organizer_id) REFERENCES users(id) ON DELETE CASCADE,
  FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE SET NULL
);

-- -------------------------------------------------------
-- SEAT TYPES (e.g. VIP, General, Student)
-- -------------------------------------------------------
CREATE TABLE IF NOT EXISTS seat_types (
  id          INT AUTO_INCREMENT PRIMARY KEY,
  event_id    INT NOT NULL,
  type_name   VARCHAR(80) NOT NULL,
  price       DECIMAL(10,2) NOT NULL DEFAULT 0.00,
  total_seats INT NOT NULL DEFAULT 50,
  booked_seats INT NOT NULL DEFAULT 0,
  FOREIGN KEY (event_id) REFERENCES events(id) ON DELETE CASCADE
);

-- -------------------------------------------------------
-- BOOKINGS
-- -------------------------------------------------------
CREATE TABLE IF NOT EXISTS bookings (
  id            INT AUTO_INCREMENT PRIMARY KEY,
  user_id       INT NOT NULL,
  event_id      INT NOT NULL,
  seat_type_id  INT DEFAULT NULL,
  quantity      INT NOT NULL DEFAULT 1,
  total_amount  DECIMAL(10,2) NOT NULL DEFAULT 0.00,
  status        ENUM('pending','confirmed','cancelled') DEFAULT 'confirmed',
  payment_method VARCHAR(50) DEFAULT 'mock_payment',
  booked_at     DATETIME DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (user_id)      REFERENCES users(id)      ON DELETE CASCADE,
  FOREIGN KEY (event_id)     REFERENCES events(id)     ON DELETE CASCADE,
  FOREIGN KEY (seat_type_id) REFERENCES seat_types(id) ON DELETE SET NULL
);

-- -------------------------------------------------------
-- TICKETS
-- -------------------------------------------------------
CREATE TABLE IF NOT EXISTS tickets (
  id          INT AUTO_INCREMENT PRIMARY KEY,
  booking_id  INT NOT NULL,
  user_id     INT NOT NULL,
  event_id    INT NOT NULL,
  ticket_code VARCHAR(60) NOT NULL UNIQUE,
  seat_number VARCHAR(20) DEFAULT NULL,
  status      ENUM('active','used','cancelled') DEFAULT 'active',
  issued_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (booking_id) REFERENCES bookings(id) ON DELETE CASCADE,
  FOREIGN KEY (user_id)    REFERENCES users(id)    ON DELETE CASCADE,
  FOREIGN KEY (event_id)   REFERENCES events(id)   ON DELETE CASCADE
);

-- Indexes for performance
CREATE INDEX idx_events_status   ON events(status);
CREATE INDEX idx_events_date     ON events(event_date);
CREATE INDEX idx_bookings_user   ON bookings(user_id);
CREATE INDEX idx_bookings_event  ON bookings(event_id);
CREATE INDEX idx_tickets_code    ON tickets(ticket_code);
