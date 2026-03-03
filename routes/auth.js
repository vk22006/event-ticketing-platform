const express = require('express');
const router = express.Router();
const authController = require('../controllers/authController');
const { requireGuest } = require('../middleware/authMiddleware');

router.get('/register', requireGuest, authController.showRegister);
router.post('/register', requireGuest, authController.register);
router.get('/login', requireGuest, authController.showLogin);
router.post('/login', requireGuest, authController.login);
router.get('/logout', authController.logout);

module.exports = router;
