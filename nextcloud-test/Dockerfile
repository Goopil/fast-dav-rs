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
# The entrypoint sees the installed instance and skips installation.
RUN php occ maintenance:install \
      --database sqlite --database-name nextcloud \
      --admin-user admin --admin-pass admin-password \
      --admin-email admin@example.local \
 && OC_PASS=fixture-dav-password php occ user:add --password-from-env test
