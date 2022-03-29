// This fields should never exist outside of this module
local objTypeField = 'magic_objType_' + std.md5(std.toString(_.deployment.deployedAt));
local hayaModuleSymbol = 'magic_module_' + std.md5(std.toString(_.deployment.deployedAt));
local hayaObjectSymbol = 'magic_object_' + std.md5(std.toString(_.deployment.deployedAt));

{
  local haya = self,
  local nativeHelmTemplate = std.native('haya.helmTemplate'),

  dockerBuild: std.native('haya.dockerBuild'),

  helmTemplate(name, package, values, purifier=function(key, value) value):
    nativeHelmTemplate(
      name, package, values,
      purifier = function(key, value) handleHelmHooks(purifier(key, value)) tailstrict,
      namespace = _.deployment.namespace,
    ) tailstrict,
  importYamlDir(name): nativeImportYamlDir(name) tailstrict,
  alwaysRecreate(value):: value {
    metadata+: {
      name+: '-' + _.deployment.deployedAt,
    },
  },
  hashObject(object): std.md5(std.manifestJsonEx(object, '  ')),
  ifChangedRecreate(dependencies, object):: object {
    metadata+: {
      annotations+: {
        ['hayasaka.delta.rocks/ifChangedRecreate-' + haya.hashObject(dependencies)]: '',
      },
    },
  },
  module(name):: assert std.isString(name) : 'module name should be string'; {
    [objTypeField]: hayaModuleSymbol,
    moduleName: name,
  },
  isModule(obj): obj[objTypeField] == hayaModuleSymbol,
  object(name): {
    [objTypeField]: hayaObjectSymbol,
    objectName: name,
  },
  mapModule(maybeModule, objectMapper): if haya.isModule(maybeModule) then {
    [name]: mapModule(maybeModule[name], objectMapper)
    for name in std.objectFields(maybeModule)
  } + haya.module(maybeModule.moduleName) else objectMapper(maybeModule) + haya.object(maybeModule.objectName),

  local fixBadFieldMixin(obj, field, fixer) = {
    [if std.objectHas(obj, field) then field else null]: fixer(obj[field]),
  },
  local fixMissingFieldMixin(obj, field, defaultValue) = {
    [if std.objectHas(obj, field) then null else field]: defaultValue,
  },

  local fixPort(port) = port + fixMissingFieldMixin(port, 'protocol', 'TCP'),
  local fixRequestsLimits(limits) = limits {
    [if std.objectHas(limits, 'cpu') then 'cpu' else null]: if std.isNumber(limits.cpu) then '' + limits.cpu else limits.cpu,
  },
  local fixResources(resources) = if resources == null then null else resources {
    [if std.objectHas(resources, 'requests') then 'requests' else null]: fixRequestsLimits(resources.requests),
    [if std.objectHas(resources, 'limits') then 'limits' else null]: fixRequestsLimits(resources.limits),
  },
  local fixContainer(container) = container {
    ports: if std.objectHas(container, 'ports') then std.map(fixPort, container.ports) else [],
    [if std.objectHas(container, 'resources') then 'resources' else null]: fixResources(container.resources),
  },
  local fixDeployment(deployment) = deployment {
    spec+: {
      template+: {
        spec+: {
          containers: std.map(fixContainer, super.containers),
        },
      },
    },
  },
  local fixStatefulSet(ss) = ss {
    spec+: {
      template+: {
        spec+: {
          containers: std.map(fixContainer, super.containers),
        },
      },
    },
  },
  local fixService(service) = service {
    spec+: {
      ports: std.map(fixPort, super.ports),
    },
  },
  local fixResource(resource) = resource + fixBadFieldMixin(resource, 'targetAverageValue', function(value) '' + value),
  local fixMetric(metric) = metric {
    resource+: fixResource(metric.resource),
  },
  local fixHPA(hpa) = hpa {
    spec+: {
      metrics: if 'metrics' in super then std.map(fixMetric, super.metrics) else null,
    },
  },

  local fixServerSideApply(value) = if value == null then null
  else if value.apiVersion == 'apps/v1' && value.kind == 'Deployment' then fixDeployment(value)
  else if value.apiVersion == 'apps/v1' && value.kind == 'StatefulSet' then fixStatefulSet(value)
  else if value.apiVersion == 'v1' && value.kind == 'Service' then fixService(value)
  else if value.apiVersion == 'autoscaling/v2beta1' && value.kind == 'HorizontalPodAutoscaler' then fixHPA(value)
  else value,

  local getHelmHooks(value) = if value == null then []
  else if 'annotations' in value.metadata && value.metadata.annotations != null && 'helm.sh/hook' in value.metadata.annotations then std.split(value.metadata.annotations['helm.sh/hook'], ',')
  else [],

  local contains(array, item) = std.length(std.find(item, array)) != 0,

  local shouldHandleAsHook(value) = value.apiVersion == 'batch/v1' && value.kind == 'Job'
                                    || value.apiVersion == 'v1' && value.kind == 'Pod',
  local handleHelmHooks(value) = if value == null then null
  // Only handle jobs
  else if !shouldHandleAsHook(value) then value
  else local hooks = getHelmHooks(value);
       // Test are skipped for now
       if contains(hooks, 'test') then null
  // Hayasaka has its own object sorting, and we can't to wait for
  // tasks to complete, so we will just bail out on post hooks
  else if contains(hooks, 'pre-delete') || contains(hooks, 'post-delete') || contains(hooks, 'pre-rollback') || contains(hooks, 'post-rollback') then error 'can\'t use "' + std.join(', ', hooks) + '" hooks with hayasaka, design your tasks as stateless'
  // This task seems to be idempotent, so we are able to just
  // always recreate it
  else if contains(hooks, 'pre-upgrade') || contains(hooks, 'post-upgrade') || contains(hooks, 'pre-install') || contains(hooks, 'post-install') then alwaysRecreate(value)
  else value,

  local nativeImportYamlDir = std.native('haya.importYamlDir'),
  _isObject(obj):: local keys = std.objectFields(obj); std.length(std.find('apiVersion', keys)) + std.length(std.find('kind', keys)) + std.length(std.find('metadata', keys)) == 3,
  _isNamespaced:: std.native('haya.isNamespaced'),
  _setObjectNamespace(obj, ns):: if !('namespace' in obj.metadata) && $._isNamespaced(obj.apiVersion, obj.kind) then obj { metadata+: { namespace: ns } } else obj,
  setNamespace(obj, ns):: if !std.isObject(obj) then error 'top level output should be object' else if $._isObject(obj) then $._setObjectNamespace(obj, ns) else {
    [key]: $.setNamespace(obj[key], ns)
    for key in std.objectFields(obj)
  },
}
