require('dotenv').config();
const express = require('express');
const session = require('express-session');
const path = require('path');
const fs = require('fs');

// ── Route imports ────────────────────────────────────────────
const authRoutes = require('./routes/auth');
const eventRoutes = require('./routes/events');
const bookingRoutes = require('./routes/bookings');
const ticketRoutes = require('./routes/tickets');
const dashboardRoutes = require('./routes/dashboard');
const adminRoutes = require('./routes/admin');

// ── Error handlers ───────────────────────────────────────────
const { notFound, serverError } = require('./middleware/errorHandler');

// ── Model for auth local injection ──────────────────────────
const User = require('./models/User');

const app = express();

// ── Ensure uploads directory exists ─────────────────────────
const uploadsDir = path.join(__dirname, 'public', 'uploads');
if (!fs.existsSync(uploadsDir)) fs.mkdirSync(uploadsDir, { recursive: true });

// ── View engine ──────────────────────────────────────────────
app.set('view engine', 'ejs');
app.set('views', path.join(__dirname, 'views'));

// ── Static files ─────────────────────────────────────────────
app.use(express.static(path.join(__dirname, 'public')));

// ── Body parsers ─────────────────────────────────────────────
app.use(express.urlencoded({ extended: true }));
app.use(express.json());

// ── Session ──────────────────────────────────────────────────
app.use(session({
    secret: process.env.SESSION_SECRET || 'changeme',
    resave: false,
    saveUninitialized: false,
    cookie: {
        secure: false,         // set true in production with HTTPS
        maxAge: 1000 * 60 * 60 * 24, // 24 hours
    },
}));

// ── Global template locals ───────────────────────────────────
app.use(async (req, res, next) => {
    res.locals.user = null;
    res.locals.session = req.session;

    if (req.session.userId) {
        try {
            res.locals.user = await User.findById(req.session.userId);
        } catch (_) { }
    }
    next();
});

// ── Routes ───────────────────────────────────────────────────
app.get('/', (req, res) => res.redirect('/events'));

app.use('/auth', authRoutes);
app.use('/events', eventRoutes);
app.use('/bookings', bookingRoutes);
app.use('/tickets', ticketRoutes);
app.use('/dashboard', dashboardRoutes);
app.use('/admin', adminRoutes);

// ── 404 & Error handlers (must be last) ─────────────────────
app.use(notFound);
app.use(serverError);

// ── Start ────────────────────────────────────────────────────
const PORT = process.env.PORT || 3000;
app.listen(PORT, () => {
    console.log(`\n Project running at http://localhost:${PORT}`);
    console.log(`   Environment : ${process.env.NODE_ENV || 'development'}`);
    console.log(`   DB          : ${process.env.DB_NAME}@${process.env.DB_HOST}\n`);
});

module.exports = app;
