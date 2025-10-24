#!/bin/bash

# Reset database
docker-compose exec mysql mysql -u root -proot -e "DROP DATABASE sabredav; CREATE DATABASE sabredav;"

# Re-import schema
docker-compose exec -T mysql mysql -u root -proot sabredav < sql/init.sql

# Seed with test calendar events
docker-compose exec -T mysql mysql -u root -proot sabredav < sql/seed.sql

echo "Database reset and seed complete!"