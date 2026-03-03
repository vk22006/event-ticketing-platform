const express = require('express');
const router = express.Router();
const bookingController = require('../controllers/bookingController');
const { requireAuth } = require('../middleware/authMiddleware');

router.get('/book/:eventId', requireAuth, bookingController.showBookingForm);
router.get('/payment', requireAuth, bookingController.showPayment);
router.post('/confirm', requireAuth, bookingController.confirmBooking);
router.get('/my', requireAuth, bookingController.myBookings);
router.post('/:id/cancel', requireAuth, bookingController.cancelBooking);

module.exports = router;
