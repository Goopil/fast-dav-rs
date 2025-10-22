# SabreDAV Test Environment with Nginx

This directory contains a complete SabreDAV test environment with Docker Compose.

## Setup

1. Run the setup script:
   ```bash
   ./setup.sh
   ```

2. Access SabreDAV at http://localhost:8080

## Test Credentials

- Username: test
- Password: test

## Database Management

- Reset and reseed the database:
  ```bash
  ./reset-db.sh
  ```

## Structure

- `config/` - Configuration files
- `data/` - SabreDAV application files
- `sql/` - Database initialization and seeding scripts
- `docker-compose.yml` - Docker Compose configuration
- `Dockerfile` - Custom SabreDAV Docker image with PHP-FPM
- `nginx/` - Nginx configuration and custom build with compression modules

## Features

- Nginx with gzip, Brotli, and zstd compression modules
- PHP-FPM for better performance
- MySQL database with preconfigured SabreDAV tables
- Test user and calendar pre-created
- Reset script for clean testing environment