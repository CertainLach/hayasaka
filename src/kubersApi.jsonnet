local fixBadFieldMixin(obj, field, fixer) = {
    [if std.objectHas(obj, field) then field else null]: fixer(obj[field])
};
local fixMissingFieldMixin(obj, field, defaultValue) = {
    [if std.objectHas(obj, field) then null else field]: defaultValue
};

local fixPort(port) = port + fixMissingFieldMixin(port, 'protocol', 'TCP');
local fixRequestsLimits(limits) = limits + {
    [if std.objectHas(limits, 'cpu') then 'cpu' else null]: if std.isNumber(limits.cpu) then ''+limits.cpu else limits.cpu,
};
local fixResources(resources) = if resources == null then null else resources + {
    [if std.objectHas(resources, 'requests') then 'requests' else null]: fixRequestsLimits(resources.requests),
    [if std.objectHas(resources, 'limits') then 'limits' else null]: fixRequestsLimits(resources.limits),
};
local fixContainer(container) = container + {
    ports: if std.objectHas(container, 'ports') then std.map(fixPort, container.ports) else [],
    [if std.objectHas(container, 'resources') then 'resources' else null]: fixResources(container.resources), 
};
local fixDeployment(deployment) = deployment + {
    spec+: {
        template+: {
            spec+: {
                containers: std.map(fixContainer, super.containers),
            },
        },
    },
};
local fixStatefulSet(ss) = ss + {
    spec+: {
        template+: {
            spec+: {
                containers: std.map(fixContainer, super.containers),
            },
        },
    },
};
local fixService(service) = service + {
    spec+: {
        ports: std.map(fixPort, super.ports),
    },
};
local fixResource(resource) = resource + fixBadFieldMixin(resource, 'targetAverageValue', function(value) ''+value);
local fixMetric(metric) = metric + {
    resource+: fixResource(metric.resource),
};
local fixHPA(hpa) = hpa + {
    spec+: {
        metrics: if 'metrics' in super then std.map(fixMetric, super.metrics) else null,
    },
};

local fixServerSideApply(value) = if value == null then null
else if value.apiVersion == "apps/v1" && value.kind == "Deployment" then fixDeployment(value)
else if value.apiVersion == "apps/v1" && value.kind == "StatefulSet" then fixStatefulSet(value)
else if value.apiVersion == "v1" && value.kind == "Service" then fixService(value)
else if value.apiVersion == "autoscaling/v2beta1" && value.kind == "HorizontalPodAutoscaler" then fixHPA(value)
else value;

local getHelmHook(value) = if value == null then null
else if 'annotations' in value.metadata && 'helm.sh/hook' in value.metadata.annotations then value.metadata.annotations['helm.sh/hook']
else null;

local handleHelmHooks(value) = local hook = getHelmHook(value);
if hook == 'test' then null
else value;

local nativeHelmTemplate = std.native("kubers.helmTemplate");

local helmTemplate(name, package, values, purifier = function(key, value) value) =
                nativeHelmTemplate(name, package, values, function(key, value) fixServerSideApply(purifier(key, value))) tailstrict;

{
	helmTemplate:: helmTemplate,
}