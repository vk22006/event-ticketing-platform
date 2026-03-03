exports.notFound = (req, res) => {
    res.status(404).render('error/404', { title: '404 – Page Not Found' });
};

exports.serverError = (err, req, res, next) => {
    console.error('💥 Unhandled error:', err);
    res.status(500).render('error/500', { title: '500 – Server Error' });
};
