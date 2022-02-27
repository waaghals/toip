# syntax=docker/dockerfile:1.2
FROM alpine

# WARNING: Don't do this.
# Normally you would USE the secret somehow, but never WRITE it as this will bake the secret in the image
RUN --mount=type=secret,id=password,dst=/tmp/pass cat /tmp/pass | tee /tmp/baked-in-image.txt
