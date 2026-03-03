
# Database Schema

The platform uses **5 relational tables**:

| Table        | Description                                     |
|--------------|-------------------------------------------------|
| `users`      | Stores all users with roles: user / organizer / admin |
| `categories` | Event categories (Music, Tech, Sports, etc.)   |
| `events`     | Event listings linked to organizers & categories |
| `seat_types` | Per-event seat tiers (VIP, General, Student)   |
| `bookings`   | Booking records linking users ↔ events          |
| `tickets`    | Individual tickets with unique codes, per booking |
