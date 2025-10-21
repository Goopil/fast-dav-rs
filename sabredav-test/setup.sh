#!/bin/bash

# Stop any running containers
docker-compose down

# Install Composer dependencies
docker-compose run --rm sabredav composer install

# Start services
docker-compose up -d

# Wait for services to be ready
echo "Waiting for services to start..."
sleep 10

echo "SabreDAV setup complete!"
echo "Access at http://localhost:8080"
echo "Test user: test/test"
echo "Reset database with: ./reset-db.sh"