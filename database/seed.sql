-- ============================================================
-- Seed Data for Event Lifecycle Platform
-- Run AFTER schema.sql
-- ============================================================

USE event_lifecycle;

-- -------------------------------------------------------
-- -- CATEGORIES
-- -- -------------------------------------------------------
INSERT INTO categories (name, icon) VALUES
  ('Music',        '🎵'),
  ('Technology',   '💻'),
  ('Sports',       '⚽'),
  ('Arts & Culture','🎨'),
  ('Business',     '💼'),
  ('Food & Drink', '🍕'),
  ('Education',    '📚'),
  ('Comedy',       '😂');

-- -- -------------------------------------------------------
-- -- USERS
-- -- Passwords are bcrypt hashes of the plain-text passwords shown in comments
-- -- -------------------------------------------------------

-- Admin: password = "Admin@1234"
INSERT INTO users (full_name, email, password, role) VALUES
('Super Admin', 'admin@eventpro.com',
 '$2b$10$3vN0EP1RfVFrbNNHZncc2et0rJr7eolibTZBTCBdy1l375EM/sc7y',
 'admin');

-- Organizers: password = "Organizer@1"
INSERT INTO users (full_name, email, password, role) VALUES
('Alice Organizer', 'alice@events.com',
 '$2b$10$cN22IeDbmzxIuaIb5pqDS.n4j6tIN1gKnptJO4sGZvsb9o2BaNR5S',
 'organizer'),
('Bob Productions', 'bob@events.com',
 '$2b$10$cN22IeDbmzxIuaIb5pqDS.n4j6tIN1gKnptJO4sGZvsb9o2BaNR5S',
 'organizer');

-- Regular Users: password = "User@1234"
INSERT INTO users (full_name, email, password, role) VALUES
('Charlie Brown',  'charlie@mail.com',
 '$2b$10$5PpEKodq0/8iAzDIFI7jFO4Z3dpdFGaiRkOtfasUbaOIQ/f.z6/oW',
 'user'),
('Diana Prince',   'diana@mail.com',
 '$2b$10$5PpEKodq0/8iAzDIFI7jFO4Z3dpdFGaiRkOtfasUbaOIQ/f.z6/oW',
 'user'),
('Ethan Hunt',     'ethan@mail.com',
 '$2b$10$5PpEKodq0/8iAzDIFI7jFO4Z3dpdFGaiRkOtfasUbaOIQ/f.z6/oW',
 'user');

-- -- -------------------------------------------------------
-- -- EVENTS (organizer_id 2 = Alice, 3 = Bob)
-- -- -------------------------------------------------------
INSERT INTO events (organizer_id, category_id, title, description, venue, city, event_date, registration_deadline, total_seats, available_seats, ticket_price, status) VALUES
(2, 1, 'SoundWave Music Festival 2025',
 'A three-day outdoor music festival featuring top artists across multiple stages with food courts and art installations.',
 'Marina Beach Grounds', 'Chennai', '2025-06-15 16:00:00', '2025-06-10 23:59:00',
 500, 500, 1500.00, 'published'),

(2, 2, 'TechCon India 2025',
 'India\'s premier technology conference featuring keynotes, workshops, and hackathons on AI, Cloud, and Web3.',
 'IIIT Auditorium', 'Hyderabad', '2025-07-20 09:00:00', '2025-07-15 23:59:00',
 300, 300, 999.00, 'published'),

(3, 3, 'IPL Watch Party – Finals Night',
 'Watch the IPL finals on a giant screen with fellow cricket fans! Food, drinks and prizes!',
 'Sports Arena Rooftop', 'Mumbai', '2025-05-25 19:00:00', '2025-05-24 23:59:00',
 200, 200, 499.00, 'published'),

(3, 4, 'Art & Soul Gallery Opening',
 'An exclusive preview of contemporary South Indian art from 40+ emerging artists. Includes a guided tour.',
 'The Gallery House', 'Bengaluru', '2025-06-01 18:00:00', '2025-05-30 23:59:00',
 80, 80, 0.00, 'published'),

(2, 5, 'Startup Pitch Night',
 'Top 10 funded startups pitch live to VCs and angel investors. Open to the public.',
 'WeWork Coworking Hub', 'Pune', '2025-08-05 17:00:00', '2025-08-01 23:59:00',
 150, 150, 299.00, 'published'),

(3, 6, 'Street Food Carnival',
 'Over 50 street food stalls from across India. Live cooking contests and celebrity chef appearances.',
 'Exhibition Park', 'Delhi', '2025-06-28 12:00:00', '2025-06-27 23:59:00',
 1000, 1000, 150.00, 'published'),

(2, 7, 'Data Science Bootcamp – Weekend Intensive',
 'Two-day hands-on bootcamp covering Python, ML basics, data visualisation, and real-world projects.',
 'Online (Zoom)', 'Online', '2025-07-12 10:00:00', '2025-07-10 23:59:00',
 100, 100, 2499.00, 'published'),

(3, 8, 'Stand-Up Comedy Showcase',
 'Laugh your evening away with 6 top stand-up comedians. 18+ event. Beverages included.',
 'The Comedy Store', 'Mumbai', '2025-06-07 20:00:00', '2025-06-06 23:59:00',
 120, 120, 799.00, 'published'),

(2, 1, 'Jazz Under the Stars',
 'An intimate outdoor jazz evening by the lakeside with craft cocktails and gourmet bites.',
 'Cubbon Park Lawns', 'Bengaluru', '2025-09-13 19:00:00', '2025-09-10 23:59:00',
 200, 200, 1200.00, 'published'),

(3, 2, 'Cybersecurity Summit 2025',
 'Annual summit on ethical hacking, data privacy, and enterprise security. CTF competition included.',
 'IIT Bombay', 'Mumbai', '2025-10-10 09:00:00', '2025-10-05 23:59:00',
 250, 250, 1499.00, 'draft');

-- -- -------------------------------------------------------
-- -- SEAT TYPES (for first 3 events)
-- -- -------------------------------------------------------
INSERT INTO seat_types (event_id, type_name, price, total_seats, booked_seats) VALUES
(1, 'General',   1500.00, 350, 0),
(1, 'VIP',       3500.00, 100, 0),
(1, 'Backstage', 7500.00,  50, 0),
(2, 'Standard',   999.00, 250, 0),
(2, 'VIP',       2499.00,  50, 0),
(3, 'General',    499.00, 180, 0),
(3, 'Premium',    999.00,  20, 0);
