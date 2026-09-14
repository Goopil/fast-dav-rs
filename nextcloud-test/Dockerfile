# Nextcloud fixture with the first-boot install baked at build time, pushed
# to GHCR by docker-images.yml. The entrypoint then skips installation on
# boot, so CI/local startup takes seconds instead of minutes.
# Built from the nextcloud-test/ context.
FROM nextcloud:stable-apache

ENV SQLITE_DATABASE=nextcloud \
    NEXTCLOUD_ADMIN_USER=admin \
    NEXTCLOUD_ADMIN_PASSWORD=admin-password \
    NEXTCLOUD_TRUSTED_DOMAINS=localhost

# Same admin/test credentials as nextcloud-test/docker-compose.yml.
# The official image ships the app in /usr/src/nextcloud and the entrypoint
# copies it to /var/www/html on first boot; replicate that here so occ can
# run at build time. The entrypoint then sees the installed instance and
# skips installation (and the named volume is seeded from this layer).
RUN cp -a /usr/src/nextcloud/. /var/www/html/ \
 && php occ maintenance:install \
      --database sqlite --database-name nextcloud \
      --admin-user admin --admin-pass admin-password \
      --admin-email admin@example.local \
 && OC_PASS=fixture-dav-password php occ user:add --password-from-env test \
 && chown -R www-data:root /var/www/html
