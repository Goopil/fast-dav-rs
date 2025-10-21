FROM php:8.4-fpm

# Install PHP extensions and dependencies
RUN apt-get update && apt-get install -y \
    libzip-dev \
    zip \
    unzip \
    git \
    && docker-php-ext-install pdo_mysql zip \
    && rm -rf /var/lib/apt/lists/*

# Download and install Composer
COPY --from=composer:latest /usr/bin/composer /usr/bin/composer

# Set working directory
WORKDIR /var/www/html

# Install SabreDAV via Composer
COPY composer.json .
RUN composer install

# Copy PHP configuration to disable OPcache in development
COPY php-conf/z-opcache-development.ini /usr/local/etc/php/conf.d/z-opcache-development.ini

# Expose port
EXPOSE 9000

# Start PHP-FPM
CMD ["php-fpm"]