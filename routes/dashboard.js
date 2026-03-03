const express = require('express');
const router = express.Router();
const multer = require('multer');
const path = require('path');
const userController = require('../controllers/userController');
const { requireAuth } = require('../middleware/authMiddleware');

const storage = multer.diskStorage({
    destination: (req, file, cb) => cb(null, 'public/uploads/'),
    filename: (req, file, cb) => {
        const ext = path.extname(file.originalname);
        cb(null, `avatar-${req.session.userId}-${Date.now()}${ext}`);
    },
});
const upload = multer({ storage });

router.get('/', requireAuth, userController.showDashboard);
router.get('/profile', requireAuth, userController.showProfile);
router.post('/profile', requireAuth, upload.single('avatar'), userController.updateProfile);

module.exports = router;
