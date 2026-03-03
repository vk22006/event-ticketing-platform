/**
 * Role-based access control middleware factory.
 * Usage: requireRole('admin') or requireRole('organizer', 'admin')
 */
exports.requireRole = (...roles) => (req, res, next) => {
    if (!req.session || !req.session.userId)
        return res.redirect('/auth/login');

    if (!roles.includes(req.session.role)) {
        return res.status(403).render('error/404', {
            title: 'Access Denied',
            message: 'You are not authorized to access this page.',
        });
    }
    next();
};
