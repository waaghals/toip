ARG PHP_VERSION=8.1
ARG COMPOSER_VERSION=2

FROM php:${PHP_VERSION} AS runtime
RUN apt-get -y update \
    && apt-get install -y libicu-dev \
    && docker-php-ext-configure intl \
    && docker-php-ext-install intl

FROM composer:${COMPOSER_VERSION} AS composerbinary

FROM runtime AS composer
COPY --from=composerbinary /usr/bin/composer /usr/bin/composer
