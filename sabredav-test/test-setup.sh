#!/usr/bin/env bash

echo "Testing SabreDAV setup..."

# Check if containers are running
if [[ $(docker compose ps | grep -c "Up") -eq 3 ]]; then
    echo "✅ All containers are running"
else
    echo "❌ Some containers are not running"
    docker compose ps
    exit 1
fi

# Check if database is initialized
echo "Checking database..."
docker compose exec mysql mysql -u root -proot -e "USE sabredav; SELECT COUNT(*) as user_count FROM users;" >/dev/null 2>&1
if [[ $? -eq 0 ]]; then
    echo "✅ Database is initialized"
else
    echo "❌ Database initialization failed"
    exit 1
fi

# Check if test user exists
USER_COUNT=$(docker compose exec mysql mysql -u root -proot -e "USE sabredav; SELECT COUNT(*) FROM users WHERE username='test';" -sN)
if [[ $USER_COUNT -eq 1 ]]; then
    echo "✅ Test user exists"
else
    echo "❌ Test user not found"
    exit 1
fi

# Check if calendar exists
CAL_COUNT=$(docker compose exec mysql mysql -u root -proot -e "USE sabredav; SELECT COUNT(*) FROM calendars;" -sN)
if [[ $CAL_COUNT -eq 1 ]]; then
    echo "✅ Default calendar created"
else
    echo "❌ Default calendar not found"
    exit 1
fi

# Check if addressbook exists
BOOK_COUNT=$(docker compose exec mysql mysql -u root -proot -e "USE sabredav; SELECT COUNT(*) FROM addressbooks;" -sN)
if [[ $BOOK_COUNT -ge 1 ]]; then
    echo "✅ Default addressbook created"
else
    echo "❌ Default addressbook not found"
    exit 1
fi

echo "✅ SabreDAV setup is ready!"
echo "Access at http://localhost:8080"
echo "Test credentials: test/test"
