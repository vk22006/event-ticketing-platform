10211CS224 - Full Stack Application Development
# End-to-End Event Lifecycle & Ticketing Platform

A full-stack web application for managing events from creation to ticket delivery. EventPro supports three user roles — **Attendees**, **Organizers**, and **Admins** — each with their own dedicated dashboard and feature set.

---

## Features

### Attendees
- Browse and search published events by category, city, and date
- Book tickets with seat-type selection (e.g. VIP, General, Student)
- View and download personal tickets with unique ticket codes
- Manage bookings and cancel if needed

### Organizers
- Create, edit, and publish events with banner image uploads
- Define multiple seat types with individual pricing
- Track bookings and revenue per event
- Cancel events and manage event lifecycle (Draft → Published → Completed)

### Admins
- View all users, organizers, and events platform-wide
- Promote users to organizer or admin roles
- Monitor overall platform statistics

---

## Tech Stack

| Layer       | Technology                          |
|-------------|-------------------------------------|
| Runtime     | Node.js                             |
| Framework   | Express.js v5                       |
| View Engine | EJS (Embedded JavaScript Templates) |
| Database    | MySQL 8 via `mysql2`                |
| Auth        | `express-session` + `bcryptjs`      |
| File Upload | `multer`                            |
| IDs         | `uuid`                              |
| Config      | `dotenv`                            |


---

## Getting Started

### Prerequisites
- [Node.js](https://nodejs.org/) v18+
- [MySQL](https://www.mysql.com/) 8.0+

### 1. Clone the repository

```bash
git clone https://github.com/your-username/End-to-end-Event-Lifecycle.git
cd End-to-end-Event-Lifecycle
```

### 2. Install dependencies

```bash
npm install
```

### 3. Configure environment variables

Create a `.env` file in the project root (or rename the existing one):

```env
PORT=3000
DB_HOST=localhost
DB_PORT=3306
DB_USER=root
DB_PASSWORD=your_mysql_password
DB_NAME=event_lifecycle
SESSION_SECRET=your_super_secret_key
NODE_ENV=development
```

### 4. Set up the database

Open **MySQL Workbench** (or any MySQL client) and run the following scripts in order:

```bash
# 1. Create tables
source database/schema.sql

# 2. (Optional) Load sample data
source database/seed.sql
```

Or paste the contents of `database/schema.sql` and `database/seed.sql` directly into MySQL Workbench and execute.

### 5. Start the application

```bash
# Production
npm start

# Development (auto-restart on file changes)
npm run dev
```

Open your browser and navigate to:
```
http://localhost:3000
```

---

## Default Roles & Seeded Accounts

After running `seed.sql`, the following test accounts are available:

| Role       | Email                    | Password   |
|------------|--------------------------|------------|
| Admin      | `admin@eventpro.com`     | `admin123` |
| Organizer  | `organizer@eventpro.com` | `org123`   |
| Attendee   | `user@eventpro.com`      | `user123`  |

> ⚠️ **Change all passwords before deploying to production.**

---

## Routes Overview

| Method | Route                    | Description                        |
|--------|--------------------------|------------------------------------|
| GET    | `/events`                | Browse all published events        |
| GET    | `/events/:id`            | Event detail page                  |
| POST   | `/bookings`              | Book tickets for an event          |
| GET    | `/tickets`               | View my tickets                    |
| GET    | `/dashboard`             | User / Organizer dashboard         |
| GET    | `/admin`                 | Admin control panel                |
| POST   | `/auth/register`         | Register a new account             |
| POST   | `/auth/login`            | Login                              |
| GET    | `/auth/logout`           | Logout                             |

---

## Security Notes

- Passwords are hashed with **bcryptjs** before storage.
- All protected routes use session-based authentication middleware.
- Role-based access control prevents unauthorized access to organizer and admin areas.
- For production: set `cookie.secure: true` and use HTTPS.

---

<!-- ## 📜 License

ISC License — see [LICENSE](LICENSE) for details. -->
