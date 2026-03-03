const Ticket = require('../models/Ticket');

exports.viewTicket = async (req, res) => {
    try {
        const ticket = await Ticket.findById(req.params.id);
        if (!ticket)
            return res.status(404).render('error/404', { title: 'Ticket Not Found' });

        // Only the ticket owner can view
        if (ticket.user_id !== req.session.userId && req.session.role !== 'admin')
            return res.status(403).render('error/404', { title: 'Forbidden' });

        res.render('ticket/view', {
            title: `Ticket – ${ticket.ticket_code}`,
            ticket,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.printTicket = async (req, res) => {
    try {
        const ticket = await Ticket.findById(req.params.id);
        if (!ticket) return res.status(404).render('error/404', { title: 'Ticket Not Found' });
        if (ticket.user_id !== req.session.userId && req.session.role !== 'admin')
            return res.status(403).render('error/404', { title: 'Forbidden' });

        res.render('ticket/print', {
            title: `Print Ticket – ${ticket.ticket_code}`,
            ticket, layout: false,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.myTickets = async (req, res) => {
    try {
        const tickets = await Ticket.findByUser(req.session.userId);
        res.render('dashboard/my-tickets', {
            title: 'My Tickets',
            tickets,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.validateTicket = async (req, res) => {
    try {
        const { code } = req.query;
        const ticket = await Ticket.findByCode(code);
        res.render('ticket/validate', {
            title: 'Validate Ticket',
            ticket, code,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};
