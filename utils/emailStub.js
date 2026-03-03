/**
 * Email Stub – logs "sent" emails to console.
 * Replace with nodemailer in production.
 */

exports.sendBookingConfirmation = (booking, tickets) => {
    console.log('\n📧 ─── EMAIL STUB ─────────────────────────────');
    console.log(`   To:      ${booking.email}`);
    console.log(`   Subject: Booking Confirmed – ${booking.event_title}`);
    console.log(`   Booking: #${booking.id} | Amount: ₹${booking.total_amount}`);
    tickets.forEach((t, i) => {
        console.log(`   Ticket ${i + 1}: ${t.ticket_code}`);
    });
    console.log('─────────────────────────────────────────────\n');
};

exports.sendCancellationNotice = (booking) => {
    console.log('\n📧 ─── EMAIL STUB ─────────────────────────────');
    console.log(`   To:      ${booking.email}`);
    console.log(`   Subject: Booking Cancelled – ${booking.event_title}`);
    console.log(`   Booking: #${booking.id} has been cancelled.`);
    console.log('─────────────────────────────────────────────\n');
};
