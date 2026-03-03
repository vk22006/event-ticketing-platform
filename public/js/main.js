/* ═══════════════════════════════════════════════════════
   EventPro – Main Client-Side JavaScript (ES6)
═══════════════════════════════════════════════════════ */

'use strict';

// ── Fade-in on scroll ──────────────────────────────────────
const fadeObserver = new IntersectionObserver(
    (entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                entry.target.style.opacity = '1';
                entry.target.style.transform = 'translateY(0)';
                fadeObserver.unobserve(entry.target);
            }
        });
    },
    { threshold: 0.1 }
);

document.querySelectorAll('.card, .stat-card').forEach(el => {
    el.style.opacity = '0';
    el.style.transform = 'translateY(16px)';
    el.style.transition = 'opacity 0.4s ease, transform 0.4s ease';
    fadeObserver.observe(el);
});

// ── Active nav link highlight ──────────────────────────────
document.querySelectorAll('.navbar__nav a').forEach(link => {
    if (link.href === window.location.href) {
        link.style.color = 'var(--text-primary)';
        link.style.background = 'var(--bg-card)';
    }
});

// ── Auto-dismiss alerts after 5 s ─────────────────────────
document.querySelectorAll('.alert').forEach(alert => {
    setTimeout(() => {
        alert.style.transition = 'opacity 0.5s ease, max-height 0.5s ease';
        alert.style.opacity = '0';
        alert.style.maxHeight = '0';
        alert.style.overflow = 'hidden';
        alert.style.padding = '0';
        alert.style.margin = '0';
    }, 5000);
});

// ── Confirm-before-submit helper for delete/cancel forms ──
document.querySelectorAll('form[data-confirm]').forEach(form => {
    form.addEventListener('submit', e => {
        if (!confirm(form.dataset.confirm)) e.preventDefault();
    });
});

// ── Seat label active border (booking page) ────────────────
document.querySelectorAll('[id^="seatLabel"]').forEach((label, _, all) => {
    label.addEventListener('click', () => {
        all.forEach(l => l.style.borderColor = 'var(--border)');
        label.style.borderColor = 'var(--accent)';
    });
});

// ── Payment option active border ───────────────────────────
document.querySelectorAll('.pay-opt').forEach((opt, idx, all) => {
    opt.addEventListener('click', () => {
        all.forEach(o => o.style.borderColor = 'var(--border)');
        opt.style.borderColor = 'var(--accent)';
    });
    if (idx === 0) opt.style.borderColor = 'var(--accent)';
});

// ── Table row hover highlight ──────────────────────────────
document.querySelectorAll('.table tbody tr').forEach(row => {
    row.addEventListener('mouseenter', () => row.style.background = 'rgba(124,92,252,0.06)');
    row.addEventListener('mouseleave', () => row.style.background = '');
});

// ── Filter form auto-submit on select change ───────────────
const filterForm = document.getElementById('filterForm');
if (filterForm) {
    filterForm.querySelectorAll('select').forEach(sel => {
        sel.addEventListener('change', () => filterForm.submit());
    });
}

// ── Smooth scroll for anchor links ────────────────────────
document.querySelectorAll('a[href^="#"]').forEach(a => {
    a.addEventListener('click', e => {
        const el = document.querySelector(a.getAttribute('href'));
        if (el) { e.preventDefault(); el.scrollIntoView({ behavior: 'smooth' }); }
    });
});
