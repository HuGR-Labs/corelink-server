// worker/src/lib/openapi_devenv.ts
// Served at GET /openapi.json

export const devenvOpenApiSpec = {
  openapi: "3.1.0",
  info: {
    title: "CoreLink DevEnv API",
    version: "1.0.0",
    description: "Persistent cloud development environments with browser, terminal, and editor",
  },
  servers: [
    { url: "https://corelink-api.humangr.com/v1", description: "Production" },
    { url: "https://api.staging.corelink.humangr.com/v1", description: "Staging" },
  ],
  components: {
    securitySchemes: {
      bearerAuth: {
        type: "http",
        scheme: "bearer",
        bearerFormat: "JWT",
      },
    },
    schemas: {
      Devenv: {
        type: "object",
        description: "DevEnv ID equals tenant UUID (1 DevEnv per tenant today).",
        properties: {
          devenv_id: {
            type: "string",
            format: "uuid",
            readOnly: true,
            description: "Equals the tenant UUID.",
          },
          workspace_name: { type: "string", nullable: true },
          profile_name: { type: "string", nullable: true },
          tier: { type: "string", enum: ["standard-2", "standard-4", "power-8", "ultra-16"], default: "standard-4" },
          status: { type: "string", enum: ["starting", "running", "stopping", "stopped", "errored"] },
          created_at: { type: "integer", format: "int64", nullable: true },
          started_at: { type: "integer", format: "int64", nullable: true },
          ports: { type: "array", items: { type: "integer" } },
        },
      },
      DevenvCreated: {
        type: "object",
        description: "Returned by POST /v1/customer/devenv (201).",
        properties: {
          devenv_id: { type: "string", format: "uuid", description: "Equals the tenant UUID." },
          workspace_name: { type: "string" },
          profile_name: { type: "string" },
          tier: { type: "string", enum: ["standard-2", "standard-4", "power-8", "ultra-16"], default: "standard-4" },
          status: { type: "string", enum: ["starting"] },
          vnc_url: { type: "string", format: "uri" },
          tty_url: { type: "string", format: "uri" },
          code_url: { type: "string", format: "uri" },
        },
        required: ["devenv_id", "workspace_name", "profile_name", "status", "vnc_url", "tty_url", "code_url"],
      },
      CreateDevenvRequest: {
        type: "object",
        required: ["workspace_name"],
        properties: {
          workspace_name: { type: "string", maxLength: 128, pattern: "^[a-zA-Z0-9_-]+$" },
          profile_name: { type: "string", maxLength: 128, default: "browser-profile" },
          tier: { type: "string", enum: ["standard-2", "standard-4", "power-8", "ultra-16"], default: "standard-4", description: "Hardware capacity tier." },
        },
      },
      ResizeRequest: {
        type: "object",
        required: ["width", "height"],
        properties: {
          width: { type: "integer", minimum: 1, maximum: 8192 },
          height: { type: "integer", minimum: 1, maximum: 8192 },
        },
      },
      SnapshotRequest: {
        type: "object",
        properties: {
          force: { type: "boolean", default: false },
        },
      },
      DevenvStatus: {
        type: "object",
        properties: {
          status: { type: "string", enum: ["starting", "running", "stopping", "stopped", "errored"] },
          workspace_name: { type: "string", nullable: true },
          profile_name: { type: "string", nullable: true },
          tier: { type: "string", enum: ["standard-2", "standard-4", "power-8", "ultra-16"], default: "standard-4" },
          created_at: { type: "integer", format: "int64", nullable: true },
          started_at: { type: "integer", format: "int64", nullable: true },
          uptime_ms: { type: "integer", nullable: true },
          container_alive: { type: "boolean" },
          ports: {
            type: "array",
            items: {
              type: "object",
              properties: {
                port: { type: "integer" },
                healthy: { type: "boolean" },
              },
            },
          },
          ws_connections: { type: "integer" },
          health_check_failures: { type: "integer" },
          last_check_at: { type: "integer" },
        },
        required: ["status", "container_alive", "ports", "ws_connections", "health_check_failures", "last_check_at"],
      },
      DevenvList: {
        type: "object",
        description: "List response. 0 or 1 DevEnvs per tenant.",
        properties: {
          devenvs: { type: "array", maxItems: 1, items: { $ref: "#/components/schemas/Devenv" } },
        },
        required: ["devenvs"],
      },
      Error: {
        type: "object",
        required: ["error"],
        properties: {
          error: { type: "string" },
        },
      },
    },
    responses: {
      Unauthorized: {
        description: "Missing or invalid authentication",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      BadRequest: {
        description: "Invalid request payload",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      QuotaExceeded: {
        description: "DevEnv quota exceeded for tier",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      NotFound: {
        description: "DevEnv not found",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      Conflict: {
        description: "Snapshot already in progress",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      ServiceUnavailable: {
        description: "DevEnv upstream (DO) unavailable",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
    },
  },
  paths: {
    "/v1/customer/devenv": {
      get: {
        summary: "List DevEnvs for tenant (0 or 1)",
        security: [{ bearerAuth: [] }],
        responses: {
          "200": {
            description: "List of DevEnvs (0 or 1 per tenant)",
            content: { "application/json": { schema: { $ref: "#/components/schemas/DevenvList" } } },
          },
          "401": { $ref: "#/components/responses/Unauthorized" },
        },
      },
      post: {
        summary: "Create and launch a new DevEnv session",
        security: [{ bearerAuth: [] }],
        requestBody: {
          required: true,
          content: { "application/json": { schema: { $ref: "#/components/schemas/CreateDevenvRequest" } } },
        },
        responses: {
          "201": {
            description: "DevEnv session launched",
            content: { "application/json": { schema: { $ref: "#/components/schemas/DevenvCreated" } } },
          },
          "400": { $ref: "#/components/responses/BadRequest" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "403": { $ref: "#/components/responses/QuotaExceeded" },
          "503": { $ref: "#/components/responses/ServiceUnavailable" },
        },
      },
    },
    "/v1/customer/devenv/status": {
      get: {
        summary: "Get DevEnv operational status and health",
        security: [{ bearerAuth: [] }],
        responses: {
          "200": {
            description: "Operational status",
            content: { "application/json": { schema: { $ref: "#/components/schemas/DevenvStatus" } } },
          },
          "401": { $ref: "#/components/responses/Unauthorized" },
        },
      },
    },
    "/v1/customer/devenv/stop": {
      post: {
        summary: "Gracefully stop the running DevEnv session",
        security: [{ bearerAuth: [] }],
        responses: {
          "200": { description: "DevEnv stopped" },
          "401": { $ref: "#/components/responses/Unauthorized" },
        },
      },
    },
    "/v1/customer/devenv/snapshot": {
      post: {
        summary: "Persist browser profile and workspace snapshot to CAS",
        security: [{ bearerAuth: [] }],
        requestBody: {
          required: false,
          content: { "application/json": { schema: { $ref: "#/components/schemas/SnapshotRequest" } } },
        },
        responses: {
          "200": { description: "Snapshot completed" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "409": { $ref: "#/components/responses/Conflict" },
        },
      },
    },
    "/v1/customer/devenv/resize": {
      post: {
        summary: "Resize desktop viewport",
        security: [{ bearerAuth: [] }],
        requestBody: {
          required: true,
          content: { "application/json": { schema: { $ref: "#/components/schemas/ResizeRequest" } } },
        },
        responses: {
          "200": { description: "Viewport resized" },
          "400": { $ref: "#/components/responses/BadRequest" },
          "401": { $ref: "#/components/responses/Unauthorized" },
        },
      },
    },
    "/v1/customer/devenv/vnc": {
      get: {
        summary: "noVNC WebSocket connection",
        security: [{ bearerAuth: [] }],
        responses: {
          "101": { description: "WebSocket upgrade to noVNC (port 6080)" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "403": { $ref: "#/components/responses/QuotaExceeded" },
          "426": { description: "Upgrade Required" },
        },
      },
    },
    "/v1/customer/devenv/tty": {
      get: {
        summary: "ttyd Terminal WebSocket connection",
        security: [{ bearerAuth: [] }],
        responses: {
          "101": { description: "WebSocket upgrade to ttyd (port 7681)" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "403": { $ref: "#/components/responses/QuotaExceeded" },
          "426": { description: "Upgrade Required" },
        },
      },
    },
    "/v1/customer/devenv/code": {
      get: {
        summary: "code-server WebSocket connection",
        security: [{ bearerAuth: [] }],
        responses: {
          "101": { description: "WebSocket upgrade to code-server (port 8080)" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "403": { $ref: "#/components/responses/QuotaExceeded" },
          "426": { description: "Upgrade Required" },
        },
      },
    },
  },
} as const;
