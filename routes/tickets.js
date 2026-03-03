const express = require('express');
const router = express.Router();
const ticketController = require('../controllers/ticketController');
const { requireAuth } = require('../middleware/authMiddleware');

router.get('/my', requireAuth, ticketController.myTickets);
router.get('/validate', requireAuth, ticketController.validateTicket);
router.get('/:id/print', requireAuth, ticketController.printTicket);
router.get('/:id', requireAuth, ticketController.viewTicket);

module.exports = router;
