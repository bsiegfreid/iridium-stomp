FROM rabbitmq:3.11-management

# Enable STOMP and ensure management is available at build time so runtime starts quickly.
RUN rabbitmq-plugins enable --offline rabbitmq_stomp rabbitmq_management

EXPOSE 5672 15672 61613

# Use the upstream entrypoint which starts rabbitmq-server
CMD ["rabbitmq-server"]
