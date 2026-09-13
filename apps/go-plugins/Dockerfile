FROM golang:1.22-alpine AS build
WORKDIR /app
COPY . .
RUN CGO_ENABLED=0 go build -o /out/host . \
 && CGO_ENABLED=0 go build -o /out/plugin-second-half ./cmd/plugin-second-half

FROM alpine:3.19
COPY --from=build /out /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/host"]
