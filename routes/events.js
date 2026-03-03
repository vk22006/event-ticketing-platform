const express = require('express');
const router = express.Router();
const multer = require('multer');
const path = require('path');
const eventController = require('../controllers/eventController');
const { requireAuth } = require('../middleware/authMiddleware');
const { requireRole } = require('../middleware/roleMiddleware');

const storage = multer.diskStorage({
    destination: (req, file, cb) => cb(null, 'public/uploads/'),
    filename: (req, file, cb) => {
        const ext = path.extname(file.originalname);
        cb(null, `event-${Date.now()}${ext}`);
    },
});
const upload = multer({ storage, limits: { fileSize: 5 * 1024 * 1024 } });

// Public routes
router.get('/', eventController.listEvents);

// Organizer-only routes (must come BEFORE /:id to avoid conflict)
router.get('/create',
    requireAuth, requireRole('organizer', 'admin'),
    eventController.showCreate);
router.post('/create',
    requireAuth, requireRole('organizer', 'admin'),
    upload.single('banner_image'),
    eventController.createEvent);

// Dynamic ID routes
router.get('/:id', eventController.showEvent);
router.get('/:id/edit',
    requireAuth, requireRole('organizer', 'admin'),
    eventController.showEdit);
router.post('/:id/edit',
    requireAuth, requireRole('organizer', 'admin'),
    upload.single('banner_image'),
    eventController.updateEvent);
router.post('/:id/delete',
    requireAuth, requireRole('organizer', 'admin'),
    eventController.deleteEvent);
router.post('/:id/toggle-status',
    requireAuth, requireRole('organizer', 'admin'),
    eventController.toggleStatus);

module.exports = router;
