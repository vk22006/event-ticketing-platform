const Event = require('../models/Event');
const Category = require('../models/Category');
const SeatType = require('../models/SeatType');
const path = require('path');

exports.listEvents = async (req, res) => {
    try {
        const { category_id, city, search, page = 1 } = req.query;
        const limit = 9;
        const offset = (page - 1) * limit;

        const [events, categories] = await Promise.all([
            Event.findAll({ status: 'published', category_id, city, search, limit, offset }),
            Category.findAll(),
        ]);

        res.render('index', {
            title: 'Discover Events',
            events, categories,
            query: req.query,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.showEvent = async (req, res) => {
    try {
        const [event, seatTypes] = await Promise.all([
            Event.findById(req.params.id),
            SeatType.findByEvent(req.params.id),
        ]);

        if (!event) return res.status(404).render('error/404', { title: 'Not Found' });

        res.render('event-detail', {
            title: event.title,
            event, seatTypes,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

// ─── Organizer-only ───────────────────────────────────────────────────────────

exports.showCreate = async (req, res) => {
    const categories = await Category.findAll();
    res.render('dashboard/create-event', {
        title: 'Create Event',
        categories, error: null,
        user: res.locals.user,
    });
};

exports.createEvent = async (req, res) => {
    try {
        const { title, description, venue, city, event_date, registration_deadline,
            total_seats, ticket_price, category_id, status } = req.body;

        if (!title || !event_date || !total_seats)
            return res.render('dashboard/create-event', {
                title: 'Create Event',
                categories: await Category.findAll(),
                error: 'Title, event date, and total seats are required.',
                user: res.locals.user,
            });

        const banner_image = req.file ? `/uploads/${req.file.filename}` : null;

        await Event.create({
            organizer_id: req.session.userId,
            category_id: category_id || null,
            title, description, venue, city, event_date, registration_deadline,
            total_seats: parseInt(total_seats),
            ticket_price: parseFloat(ticket_price) || 0,
            status: status === 'published' ? 'published' : 'draft',
            banner_image,
        });

        res.redirect('/dashboard');
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.showEdit = async (req, res) => {
    const [event, categories] = await Promise.all([
        Event.findById(req.params.id),
        Category.findAll(),
    ]);

    if (!event || event.organizer_id !== req.session.userId)
        return res.status(403).render('error/404', { title: 'Forbidden' });

    res.render('dashboard/edit-event', {
        title: 'Edit Event',
        event, categories, error: null,
        user: res.locals.user,
    });
};

exports.updateEvent = async (req, res) => {
    try {
        const event = await Event.findById(req.params.id);
        if (!event || event.organizer_id !== req.session.userId)
            return res.status(403).render('error/404', { title: 'Forbidden' });

        const { title, description, venue, city, event_date, registration_deadline,
            total_seats, ticket_price, category_id, status } = req.body;

        const fields = {
            title, description, venue, city, event_date, registration_deadline,
            total_seats: parseInt(total_seats), ticket_price: parseFloat(ticket_price),
            category_id: category_id || null, status
        };

        if (req.file) fields.banner_image = `/uploads/${req.file.filename}`;

        await Event.update(req.params.id, fields);
        res.redirect('/dashboard');
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.deleteEvent = async (req, res) => {
    const event = await Event.findById(req.params.id);
    if (!event || event.organizer_id !== req.session.userId)
        return res.status(403).json({ error: 'Forbidden' });

    await Event.delete(req.params.id);
    res.redirect('/dashboard');
};

exports.toggleStatus = async (req, res) => {
    const event = await Event.findById(req.params.id);
    if (!event || event.organizer_id !== req.session.userId)
        return res.status(403).json({ error: 'Forbidden' });

    const newStatus = event.status === 'published' ? 'draft' : 'published';
    await Event.update(req.params.id, { status: newStatus });
    res.redirect('/dashboard');
};
