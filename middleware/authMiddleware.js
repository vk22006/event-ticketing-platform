exports.requireAuth = (req, res, next) => {
    if (req.session && req.session.userId) return next();
    res.redirect('/auth/login');
};

exports.requireGuest = (req, res, next) => {
    if (req.session && req.session.userId) return res.redirect('/dashboard');
    next();
};
