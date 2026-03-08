const { Client } = require('pg');

const client = new Client({
    connectionString: "postgres://postgres:tbddb@localhost:5432/Turf_BD",
});

async function check() {
    try {
        await client.connect();
        console.log("Connected to database");

        const tables = await client.query("SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'");
        console.log("Tables:");
        tables.rows.forEach(row => console.log(`- ${row.table_name}`));

        const count = await client.query("SELECT count(*) FROM users");
        console.log(`\nUser count: ${count.rows[0].count}`);

        const users = await client.query("SELECT id, username, email FROM users");
        console.log("\nUsers:");
        users.rows.forEach(row => console.log(row));

    } catch (err) {
        console.error("Error:", err.message);
    } finally {
        await client.end();
    }
}

check();
