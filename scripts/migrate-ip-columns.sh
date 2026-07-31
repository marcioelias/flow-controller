#!/bin/bash
# Migra network_flows_v4/v6 das colunas IP String para os tipos nativos
# IPv4/IPv6 (task 10.12). Requer parada do coletor durante a cópia.
#
# Estratégia: cria tabela nova com schema nativo, INSERT SELECT, RENAME.
# Nunca é executado automaticamente pelo coletor.
set -euo pipefail

CH_URL="${CLICKHOUSE_URL:-http://localhost:8123}"
CH_USER="${CLICKHOUSE_USER:-default}"
CH_PASS="${CLICKHOUSE_PASSWORD:-}"
RETENTION="${FLOW_RETENTION_DAYS:-30}"

ch() {
    curl -sS --fail-with-body "${CH_URL}/" \
        -u "${CH_USER}:${CH_PASS}" \
        --data-binary "$1"
}

echo "== FlowVision: migração de colunas IP para tipos nativos =="
echo "ClickHouse: ${CH_URL}"
echo

# Pré-checagem: schema atual
V4_TYPE=$(ch "SELECT type FROM system.columns WHERE database = currentDatabase() AND table = 'network_flows_v4' AND name = 'src_ip'")
if [ "$V4_TYPE" != "String" ]; then
    echo "network_flows_v4.src_ip já é '${V4_TYPE}' — nada a migrar."
    exit 0
fi

ROWS_V4=$(ch "SELECT count() FROM network_flows_v4")
ROWS_V6=$(ch "SELECT count() FROM network_flows_v6")
SIZE=$(ch "SELECT formatReadableSize(sum(bytes_on_disk)) FROM system.parts WHERE table IN ('network_flows_v4','network_flows_v6') AND active")

echo "Linhas v4: ${ROWS_V4} | Linhas v6: ${ROWS_V6} | Tamanho em disco: ${SIZE}"
echo "A cópia lê e regrava tudo. PARE O COLETOR antes de continuar."
read -r -p "Continuar? [digite MIGRAR] " CONFIRM
[ "$CONFIRM" = "MIGRAR" ] || { echo "Abortado."; exit 1; }

migrate_table() {
    local TABLE="$1" SRC_TYPE="$2" DST_TYPE="$3"
    echo "-- ${TABLE}: criando ${TABLE}_native"
    ch "DROP TABLE IF EXISTS ${TABLE}_native"
    ch "CREATE TABLE ${TABLE}_native (
            timestamp DateTime,
            exporter_ip IPv4,
            src_ip ${SRC_TYPE},
            dst_ip ${DST_TYPE},
            src_port UInt16,
            dst_port UInt16,
            protocol UInt8,
            src_asn UInt32,
            dst_asn UInt32,
            packets UInt64,
            bytes UInt64,
            flow_count UInt64,
            direction UInt8 DEFAULT 255
        )
        ENGINE = MergeTree()
        PARTITION BY toYYYYMMDD(timestamp)
        ORDER BY (exporter_ip, timestamp, src_ip, dst_ip, protocol)
        TTL timestamp + INTERVAL ${RETENTION} DAY
        SETTINGS index_granularity = 8192"

    echo "-- ${TABLE}: copiando dados (pode demorar)"
    ch "INSERT INTO ${TABLE}_native
        SELECT timestamp,
               toIPv4OrDefault(exporter_ip),
               to${SRC_TYPE}OrDefault(src_ip),
               to${DST_TYPE}OrDefault(dst_ip),
               src_port, dst_port, protocol, src_asn, dst_asn,
               packets, bytes, flow_count, direction
        FROM ${TABLE}"

    local OLD NEW
    OLD=$(ch "SELECT count() FROM ${TABLE}")
    NEW=$(ch "SELECT count() FROM ${TABLE}_native")
    echo "-- ${TABLE}: ${OLD} → ${NEW} linhas"
    if [ "$OLD" != "$NEW" ]; then
        echo "ERRO: contagem divergente em ${TABLE}; tabela original preservada."
        exit 1
    fi

    ch "RENAME TABLE ${TABLE} TO ${TABLE}_legacy_backup, ${TABLE}_native TO ${TABLE}"
    echo "-- ${TABLE}: concluído (backup em ${TABLE}_legacy_backup)"
}

# MVs dependem da v4 — derruba e deixa o coletor recriar no próximo start
echo "-- removendo MVs (recriadas pelo coletor no próximo start)"
ch "DROP VIEW IF EXISTS ip_hourly_tx_v4"
ch "DROP VIEW IF EXISTS ip_hourly_rx_v4"

migrate_table network_flows_v4 IPv4 IPv4
migrate_table network_flows_v6 IPv6 IPv6

echo
echo "Migração concluída. Suba o coletor e valide o dashboard."
echo "Depois de validar, libere espaço com:"
echo "  DROP TABLE network_flows_v4_legacy_backup"
echo "  DROP TABLE network_flows_v6_legacy_backup"
