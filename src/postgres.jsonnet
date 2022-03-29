local PostgresCluster = {
  _config:: {
    clusterName: error 'cluserName should be set',
    storage: {
      className: error 'storage.className should be set',
      size: error 'storage.size should be set',
    },
    replicas: error 'replicas should be set',
    spiloImage: 'docker.io/cacture/postgres:1',
  },
  statefulSet: {
    apiVersion: 'apps/v1beta1',
    kind: 'StatefulSet',
    metadata: {
      name: $._config.clusterName,
      labels: {
        application: 'spilo',
        'spilo-cluster': $._config.clusterName,
      },
    },
    spec: {
      replicas: $._config.replicas,
      serviceName: $._config.clusterName,
      template: {
        metadata: {
          labels: {
            application: 'spilo',
            'spilo-cluster': $._config.clusterName,
          },
          annotations: {
            'scheduler.alpha.kubernetes.io/affinity': std.manifestJsonEx(
              {
                podAntiAffinity: {
                  requiredDuringSchedulingIgnoredDuringExecution: [
                    {
                      labelSelector: {
                        matchExpressions: [
                          {
                            key: 'spilo-cluster',
                            operator: 'In',
                            values: [$._config.clusterName],
                          },
                        ],
                      },
                      topologyKey: 'kubernetes.io/hostname',
                    },
                  ],
                },
              }, '  '
            ),
          },
        },
        spec: {
          containers: [
            {
              name: $._config.clusterName,
              image: $._config.spiloImage,
              imagePullPolicy: 'IfNotPresent',
              ports: [
                {
                  containerPort: 8008,
                  protocol: 'TCP',
                },
                {
                  containerPort: 5432,
                  protocol: 'TCP',
                },
              ],
              volumeMounts: [
                {
                  mountPath: '/home/postgres/pgdata',
                  name: 'pgdata',
                },
              ],
              readinessProbe: {
                httpGet: {
                  scheme: 'HTTP',
                  path: '/readiness',
                  port: 8008,
                },
                initialDelaySeconds: 3,
                periodSeconds: 10,
                timeoutSeconds: 5,
                successThreshold: 1,
                failureThreshold: 3,
              },
              livenessProbe: {
                httpGet: {
                  scheme: 'HTTP',
                  path: '/liveness',
                  port: 8008,
                },
                initialDelaySeconds: 3,
                periodSeconds: 10,
                timeoutSeconds: 5,
                successThreshold: 1,
                failureThreshold: 3,
              },
              env: [
                {
                  name: 'DCS_ENABLE_KUBERNETES_API',
                  value: 'true',
                },
                {
                  name: 'ENABLE_WAL_PATH_COMPAT',
                  value: 'true',
                },
                // {
                //   name: 'WAL_S3_BUCKET',
                //   value: 'example-spilo-dbaas',
                // },
                // {
                //   name: 'LOG_S3_BUCKET',
                //   value: 'example-spilo-dbaas',
                // },
                {
                  name: 'BACKUP_SCHEDULE',
                  value: '00 01 * * *',
                },
                {
                  name: 'LABELS',
                  value: std.manifestJsonEx({
                    allication: 'spilo',
                  }),
                },
                {
                  name: 'KUBERNETES_ROLE_LABEL',
                  value: 'role',
                },
                {
                  name: 'SPILO_CONFIGURATION',
                  value: std.manifestJsonEx({
                    postgresql: {},
                    bootstrap: {
                      initdb: [{ 'auth-host': 'md5' }, { 'auth-local': 'trust' }],
                      users: { postgres: { password: '', options: ['CREATEDB', 'NOLOGIN'] } },
                      dcs: {},
                    },
                  }),
                },
                {
                  name: 'POD_IP',
                  valueFrom: {
                    fieldRef: {
                      apiVersion: 'v1',
                      fieldPath: 'status.podIP',
                    },
                  },
                },
                {
                  name: 'POD_NAMESPACE',
                  valueFrom: {
                    fieldRef: {
                      apiVersion: 'v1',
                      fieldPath: 'metadata.namespace',
                    },
                  },
                },
                {
                  name: 'PGPASSWORD_SUPERUSER',
                  valueFrom: {
                    secretKeyRef: {
                      name: $._config.clusterName,
                      key: 'superuser-password',
                    },
                  },
                },
                {
                  name: 'PGUSER_ADMIN',
                  value: 'superadmin',
                },
                {
                  name: 'PGPASSWORD_ADMIN',
                  valueFrom: {
                    secretKeyRef: {
                      name: $._config.clusterName,
                      key: 'admin-password',
                    },
                  },
                },
                {
                  name: 'PGROOT',
                  value: '/home/postgres/pgdata/pgroot',
                },
                {
                  name: 'PGUSER_STANDBY',
                  value: 'standby',
                },
                {
                  name: 'PGPASSWORD_STANDBY',
                  valueFrom: {
                    secretKeyRef: {
                      name: $._config.clusterName,
                      key: 'replication-password',
                    },
                  },
                },
                {
                  name: 'SCOPE',
                  value: $._config.clusterName,
                },
              ],
            },
          ],

          terminationGracePeriodSeconds: 0,
        },
      },
      volumeClaimTemplates: [
        {
          metadata: {
            labels: {
              application: 'spilo',
              'spilo-cluster': $._config.clusterName,
            },
            name: 'pgdata',
          },
          spec: {
            accessModes: [
              'ReadWriteOnce',
            ],
            storageClassName: $._config.storage.className,
            resources: {
              requests: {
                storage: $._config.storage.size,
              },
            },
          },
        },
      ],
    },
  },
  endpoints: {
    apiVersion: 'v1',
    kind: 'Endpoints',
    metadata: {
      name: $._config.clusterName,
      labels: {
        application: 'spilo',
        'spilo-cluster': $._config.clusterName,
      },
    },
    subsets: [],
  },
  service: {
    apiVersion: 'v1',
    kind: 'Service',
    metadata: {
      name: $._config.clusterName,
      labels: {
        application: 'spilo',
        'spilo-cluster': $._config.clusterName,
      },
    },
    spec: {
      type: 'ClusterIP',
      ports: [
        {
          name: 'postgresql',
          port: 5432,
          targetPort: 5432,
        },
      ],
    },
  },
  configService: {
    apiVersion: 'v1',
    kind: 'Service',
    metadata: {
      name: $._config.clusterName + '-config',
      labels: {
        application: 'spilo',
        'spilo-cluster': $._config.clusterName,
      },
    },
    spec: {
      clusterIP: 'None',
    },
  },
  secret: {
    apiVersion: 'v1',
    kind: 'Secret',
    metadata: {
      name: $._config.clusterName,
      labels: {
        application: 'spilo',
        'spilo-cluster': $._config.clusterName,
      },
    },
    type: 'Opaque',
    data: {
      'superuser-password': std.base64('postgres'),
      'replication-password': std.base64('postgres'),
      'admin-password': std.base64('postgres'),
    },
  },
};

SpiloCluster {
  _config+: {
    clusterName: 'test',
    storage: {
      className: 'local-btrfs-subvolume',
      size: '20Gi',
    },
    replicas: 3,
  },
}
