FROM apache/activemq-artemis:latest

# Copy config overrides into etc-override; the entrypoint merges them
# after instance creation.
COPY broker.xml /var/lib/artemis-instance/etc-override/broker.xml
COPY artemis-users.properties /var/lib/artemis-instance/etc-override/artemis-users.properties
COPY artemis-roles.properties /var/lib/artemis-instance/etc-override/artemis-roles.properties

EXPOSE 61613 8161
