import psycopg2
import sys

try:
    conn = psycopg2.connect("postgres://postgres:tbddb@localhost:5432/Turf_BD")
    cur = conn.cursor()
    cur.execute("SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'")
    tables = cur.fetchall()
    print("Tables in database:")
    for table in tables:
        print(f"- {table[0]}")
    
    cur.execute("SELECT count(*) FROM users")
    count = cur.fetchone()[0]
    print(f"\nUser count: {count}")
    
    cur.execute("SELECT id, username, email FROM users")
    users = cur.fetchall()
    print("\nUsers:")
    for user in users:
        print(user)
        
    cur.close()
    conn.close()
except Exception as e:
    print(f"Error: {e}")
