const User = require('../models/User');
const Event = require('../models/Event');
const Booking = require('../models/Booking');

exports.dashboard = async (req, res) => {
    try {
        const [userCounts, totalEvents, publishedEvents, totalBookings, revenue, recentBookings] = await Promise.all([
            User.countByRole(),
            Event.count(),
            Event.count('published'),
            Booking.countTotal(),
            Booking.totalRevenue(),
            Booking.recentBookings(8),
        ]);

        const users = userCounts.reduce((acc, r) => { acc[r.role] = r.total; return acc; }, {});

        res.render('dashboard/admin', {
            title: 'Admin Dashboard',
            stats: {
                totalUsers: Object.values(users).reduce((a, b) => a + Number(b), 0),
                organizers: users.organizer || 0,
                regularUsers: users.user || 0,
                totalEvents,
                publishedEvents,
                totalBookings,
                revenue: parseFloat(revenue).toFixed(2),
            },
            recentBookings,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.listUsers = async (req, res) => {
    try {
        const users = await User.findAll();
        res.render('dashboard/admin-users', {
            title: 'Manage Users',
            users,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.listEvents = async (req, res) => {
    try {
        const events = await Event.findAll({ status: null, limit: 100, offset: 0 });
        res.render('dashboard/admin-events', {
            title: 'Manage Events',
            events,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.reports = async (req, res) => {
    try {
        const [totalBookings, revenue, recentBookings] = await Promise.all([
            Booking.countTotal(),
            Booking.totalRevenue(),
            Booking.recentBookings(20),
        ]);
        res.render('dashboard/admin-reports', {
            title: 'Reports',
            totalBookings,
            revenue: parseFloat(revenue).toFixed(2),
            recentBookings,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};
