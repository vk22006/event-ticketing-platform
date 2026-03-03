const User = require('../models/User');
const Event = require('../models/Event');
const Booking = require('../models/Booking');

exports.showDashboard = async (req, res) => {
    try {
        let data = { user: res.locals.user, title: 'Dashboard' };

        if (req.session.role === 'organizer') {
            data.events = await Event.getByOrganizer(req.session.userId);
        } else {
            data.bookings = await Booking.findByUser(req.session.userId);
        }

        res.render('dashboard/home', data);
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.showProfile = async (req, res) => {
    try {
        const profile = await User.findById(req.session.userId);
        res.render('dashboard/profile', {
            title: 'My Profile',
            profile,
            error: null,
            success: null,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.updateProfile = async (req, res) => {
    try {
        const { full_name, bio } = req.body;
        const avatar = req.file ? `/uploads/${req.file.filename}` : null;

        if (!full_name)
            return res.render('dashboard/profile', {
                title: 'My Profile',
                profile: await User.findById(req.session.userId),
                error: 'Name is required.',
                success: null,
                user: res.locals.user,
            });

        await User.updateProfile(req.session.userId, {
            full_name,
            bio: bio || null,
            avatar: avatar || (await User.findById(req.session.userId)).avatar,
        });

        req.session.name = full_name;

        res.render('dashboard/profile', {
            title: 'My Profile',
            profile: await User.findById(req.session.userId),
            error: null,
            success: 'Profile updated successfully!',
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};
