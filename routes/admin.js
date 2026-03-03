const express = require('express');
const router = express.Router();
const adminController = require('../controllers/adminController');
const { requireAuth } = require('../middleware/authMiddleware');
const { requireRole } = require('../middleware/roleMiddleware');

router.use(requireAuth, requireRole('admin'));

router.get('/dashboard', adminController.dashboard);
router.get('/users', adminController.listUsers);
router.get('/events', adminController.listEvents);
router.get('/reports', adminController.reports);

module.exports = router;
