FROM nginx:alpine

# Install build dependencies
RUN apk add --no-cache \
    zstd \
    git \
    cmake \
    make \
    g++ \
    libtool \
    automake \
    linux-headers \
    pcre-dev \
    openssl-dev \
    zlib-dev \
    libxslt-dev \
    gd-dev

# Build and install brotli library
RUN cd /tmp && \
    git clone https://github.com/google/brotli.git && \
    cd brotli && \
    mkdir out && \
    cd out && \
    cmake .. && \
    make && \
    make install

# Clone ngx_brotli module
RUN cd /tmp && \
    git clone https://github.com/google/ngx_brotli.git && \
    cd ngx_brotli && \
    git submodule update --init

# Clone headers-more-nginx-module
RUN cd /tmp && \
    git clone https://github.com/openresty/headers-more-nginx-module.git

# Download and compile Nginx with modules
RUN cd /tmp && \
    wget https://nginx.org/download/nginx-1.25.3.tar.gz && \
    tar zxf nginx-1.25.3.tar.gz && \
    cd nginx-1.25.3 && \
    export CFLAGS="-Wno-error=unterminated-string-initialization" && \
    ./configure \
        --prefix=/etc/nginx \
        --sbin-path=/usr/sbin/nginx \
        --modules-path=/usr/lib/nginx/modules \
        --conf-path=/etc/nginx/nginx.conf \
        --error-log-path=/var/log/nginx/error.log \
        --http-log-path=/var/log/nginx/access.log \
        --pid-path=/var/run/nginx.pid \
        --lock-path=/var/run/nginx.lock \
        --http-client-body-temp-path=/var/cache/nginx/client_temp \
        --http-proxy-temp-path=/var/cache/nginx/proxy_temp \
        --http-fastcgi-temp-path=/var/cache/nginx/fastcgi_temp \
        --http-uwsgi-temp-path=/var/cache/nginx/uwsgi_temp \
        --http-scgi-temp-path=/var/cache/nginx/scgi_temp \
        --with-perl_modules_path=/usr/lib/perl5/vendor_perl \
        --user=nginx \
        --group=nginx \
        --with-compat \
        --with-file-aio \
        --with-threads \
        --with-http_addition_module \
        --with-http_auth_request_module \
        --with-http_dav_module \
        --with-http_flv_module \
        --with-http_gunzip_module \
        --with-http_gzip_static_module \
        --with-http_mp4_module \
        --with-http_random_index_module \
        --with-http_realip_module \
        --with-http_secure_link_module \
        --with-http_slice_module \
        --with-http_ssl_module \
        --with-http_stub_status_module \
        --with-http_sub_module \
        --with-http_v2_module \
        --with-mail \
        --with-mail_ssl_module \
        --with-stream \
        --with-stream_realip_module \
        --with-stream_ssl_module \
        --with-stream_ssl_preread_module \
        --add-module=/tmp/ngx_brotli \
        --add-module=/tmp/headers-more-nginx-module && \
    make && \
    make install

# Create nginx directories
RUN mkdir -p /var/cache/nginx/client_temp && \
    mkdir -p /var/cache/nginx/proxy_temp && \
    mkdir -p /var/cache/nginx/fastcgi_temp && \
    mkdir -p /var/cache/nginx/uwsgi_temp && \
    mkdir -p /var/cache/nginx/scgi_temp

# Copy nginx configuration
COPY nginx.conf /etc/nginx/nginx.conf

# Expose port
EXPOSE 80

# Start nginx
CMD ["nginx", "-g", "daemon off;"]