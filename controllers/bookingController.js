const Booking = require('../models/Booking');
const Ticket = require('../models/Ticket');
const Event = require('../models/Event');
const SeatType = require('../models/SeatType');
const emailStub = require('../utils/emailStub');

exports.showBookingForm = async (req, res) => {
    try {
        const [event, seatTypes] = await Promise.all([
            Event.findById(req.params.eventId),
            SeatType.findByEvent(req.params.eventId),
        ]);

        if (!event || event.status !== 'published')
            return res.status(404).render('error/404', { title: 'Event Not Found' });

        if (event.available_seats <= 0)
            return res.render('booking/select', {
                title: 'Book Tickets',
                event, seatTypes, error: 'Sorry, this event is sold out.',
                user: res.locals.user,
            });

        res.render('booking/select', {
            title: 'Book Tickets',
            event, seatTypes, error: null,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.showPayment = async (req, res) => {
    try {
        const { eventId, seat_type_id, quantity } = req.query;
        const qty = parseInt(quantity) || 1;

        const [event, seatTypes] = await Promise.all([
            Event.findById(eventId),
            SeatType.findByEvent(eventId),
        ]);

        if (!event) return res.status(404).render('error/404', { title: 'Not Found' });

        const seatType = seatTypes.find(s => s.id == seat_type_id) || null;
        const price = seatType ? seatType.price : event.ticket_price;
        const total = price * qty;

        res.render('booking/payment', {
            title: 'Payment',
            event, seatType, quantity: qty, total,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.confirmBooking = async (req, res) => {
    try {
        const { event_id, seat_type_id, quantity } = req.body;
        const qty = parseInt(quantity) || 1;

        const hasCapacity = await Booking.checkCapacity(event_id, qty);
        if (!hasCapacity)
            return res.render('error/500', { title: 'Sold Out', message: 'Not enough seats available.' });

        const event = await Event.findById(event_id);
        const seatTypes = await SeatType.findByEvent(event_id);
        const seatType = seatTypes.find(s => s.id == seat_type_id) || null;
        const price = seatType ? seatType.price : event.ticket_price;
        const total = price * qty;

        // Create booking
        const bookingId = await Booking.create({
            user_id: req.session.userId,
            event_id,
            seat_type_id: seat_type_id || null,
            quantity: qty,
            total_amount: total,
        });

        // Decrement seats
        await Event.decrementSeats(event_id, qty);
        if (seatType) await SeatType.decrementBooked(seatType.id, qty);

        // Generate ticket(s)
        const tickets = [];
        for (let i = 0; i < qty; i++) {
            const ticket = await Ticket.create({
                booking_id: bookingId,
                user_id: req.session.userId,
                event_id,
            });
            tickets.push(ticket);
        }

        // Send email stub
        const booking = await Booking.findById(bookingId);
        emailStub.sendBookingConfirmation(booking, tickets);

        res.render('booking/confirmation', {
            title: 'Booking Confirmed!',
            booking, tickets, event,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.myBookings = async (req, res) => {
    try {
        const bookings = await Booking.findByUser(req.session.userId);
        res.render('dashboard/my-bookings', {
            title: 'My Bookings',
            bookings,
            user: res.locals.user,
        });
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};

exports.cancelBooking = async (req, res) => {
    try {
        const booking = await Booking.findById(req.params.id);
        if (!booking || booking.user_id !== req.session.userId)
            return res.status(403).render('error/404', { title: 'Forbidden' });

        await Booking.cancel(req.params.id);
        await Event.incrementSeats(booking.event_id, booking.quantity);
        await Ticket.cancelByBooking(booking.id);

        res.redirect('/bookings/my');
    } catch (err) {
        console.error(err);
        res.status(500).render('error/500', { title: 'Error' });
    }
};
