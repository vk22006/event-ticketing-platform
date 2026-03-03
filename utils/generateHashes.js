// Temporary script to generate correct bcrypt hashes for seed data
const bcrypt = require('bcryptjs');

async function main() {
    const passwords = [
        { label: 'Admin@1234', value: 'Admin@1234' },
        { label: 'Organizer@1', value: 'Organizer@1' },
        { label: 'User@1234', value: 'User@1234' },
    ];

    for (const p of passwords) {
        const hash = await bcrypt.hash(p.value, 10);
        console.log(`${p.label}:\n  ${hash}\n`);
    }
}

main();
