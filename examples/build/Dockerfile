# syntax=docker/dockerfile:1.2
FROM alpine:3

ARG WHO_BUILD_THIS
ARG FILE_NAME=/tmp/who.txt
RUN echo $WHO_BUILD_THIS > ${FILE_NAME}
