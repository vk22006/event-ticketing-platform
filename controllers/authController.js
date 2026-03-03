const User = require('../models/User');

exports.showRegister = (req, res) => {
    if (req.session.userId) return res.redirect('/');
    res.render('auth/register', { title: 'Register', error: null });
};

exports.showLogin = (req, res) => {
    if (req.session.userId) return res.redirect('/');
    res.render('auth/login', { title: 'Login', error: null });
};

exports.register = async (req, res) => {
    try {
        const { full_name, email, password, confirm_password, role } = req.body;

        if (!full_name || !email || !password)
            return res.render('auth/register', { title: 'Register', error: 'All fields are required.' });

        if (password !== confirm_password)
            return res.render('auth/register', { title: 'Register', error: 'Passwords do not match.' });

        if (password.length < 6)
            return res.render('auth/register', { title: 'Register', error: 'Password must be at least 6 characters.' });

        const existing = await User.findByEmail(email);
        if (existing)
            return res.render('auth/register', { title: 'Register', error: 'An account with this email already exists.' });

        // Only allow user/organizer self-registration (not admin)
        const safeRole = role === 'organizer' ? 'organizer' : 'user';
        const id = await User.create({ full_name, email, password, role: safeRole });

        req.session.userId = id;
        req.session.role = safeRole;
        req.session.name = full_name;

        res.redirect('/dashboard');
    } catch (err) {
        console.error('Register error:', err);
        res.render('auth/register', { title: 'Register', error: 'Server error. Please try again.' });
    }
};

exports.login = async (req, res) => {
    try {
        const { email, password } = req.body;

        if (!email || !password)
            return res.render('auth/login', { title: 'Login', error: 'Email and password are required.' });

        const user = await User.findByEmail(email);
        if (!user)
            return res.render('auth/login', { title: 'Login', error: 'Invalid email or password.' });

        const valid = await User.verifyPassword(password, user.password);
        if (!valid)
            return res.render('auth/login', { title: 'Login', error: 'Invalid email or password.' });

        req.session.userId = user.id;
        req.session.role = user.role;
        req.session.name = user.full_name;

        res.redirect(user.role === 'admin' ? '/admin/dashboard' : '/dashboard');
    } catch (err) {
        console.error('Login error:', err);
        res.render('auth/login', { title: 'Login', error: 'Server error. Please try again.' });
    }
};

exports.logout = (req, res) => {
    req.session.destroy(() => res.redirect('/'));
};
