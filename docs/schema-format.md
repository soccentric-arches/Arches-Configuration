# Schema format

A **schema file** is a JSON document dropped into
`/usr/share/webconfig/schemas/*.schema.json` (installed by the
`webconfig-schemas` Yocto recipe and its bbappends). Each schema does two jobs
at once (spec §3):

1. **Drives the frontend renderer** — the generic `schema-form.js` builds a form
   from the `groups[].fields[]` descriptor. **Adding a setting is a schema file
   edit — zero UI code, zero daemon rebuild** for schema-driven app settings.
2. **Drives backend validation** — the `validate` subschema is compiled by the
   `jsonschema` crate and applied to the config slice on commit. The API is the
   security boundary, so every input is re-validated server-side regardless of
   client validation.

## Top-level shape

```jsonc
{
  "id": "app:example",          // section id; "app:<id>" slices live in config.app_settings
  "title": "Example customer app",
  "groups": [ { /* SchemaGroup */ } ],
  "validate": { /* JSON Schema draft-07 for the same object */ }
}
```

- `id` keys the slice. Section ids like `system`/`network` validate the typed
  config subsection; `app:<id>` ids validate a slice of `config.app_settings`.
- `validate` is optional. When absent, the whole descriptor is treated as
  render-only (no server validation for that slice).

## SchemaGroup

```jsonc
{ "id": "general", "title": "General", "fields": [ /* SchemaField */ ] }
```

## SchemaField

```jsonc
{
  "id": "site_id",            // key in the settings object
  "title": "Site ID",         // label
  "field_type": "string",     // string | int | bool | enum | secret
  "required": true,
  "options": [ { "value": "info", "label": "Info" } ], // enum only
  "validation": {             // ValidationRules; applied client + server
    "min": 1, "max": 3600,    // int bounds
    "min_len": 1, "max_len": 32,
    "pattern": "^[A-Za-z0-9_-]+$"
  },
  "help": "Unique identifier for this installation site.",
  "apply_hook": { "kind": "restart", "target": "my-app.service" }
}
```

### `field_type` (fixed set)

| Type | Renders as | Reads back as |
|---|---|---|
| `string` | text input | string |
| `int` | number input (step 1) | integer (or null if non-integer) |
| `bool` | checkbox | boolean |
| `enum` | select of `options` | the chosen `value` |
| `secret` | password input | string (never returned in backups by default) |

### `ValidationRules`

| Key | Applies to | Meaning |
|---|---|---|
| `min`, `max` | `int` | inclusive integer bounds |
| `min_len`, `max_len` | `string`, `secret` | length bounds |
| `pattern` | `string`, `secret` | ECMAScript/RE2 regex the value must match |

### `apply_hook` (optional)

Declares a side effect to run after apply: `{ "kind": "restart" | "reload" | "sighup", "target": "<unit or process>" }`. The daemon interprets this on the real root.

## Example

See [`schemas/app-example.schema.json`](../schemas/app-example.schema.json) for a
complete, validated example (string + int + enum + bool + secret across two
groups with a draft-07 `validate` block).

## How the frontend uses it

`GET /api/v1/schemas/:section` returns `{id, descriptor}`. `schema-form.js`
calls `SchemaForm.render(descriptor, values)` → `{element, getValue, validate}`.
On save the page PUTs `getValue()` to `/api/v1/apps/:id/settings`; the store
re-validates against `validate` before persisting.

## How a customer adds a setting

1. Write `my-app.schema.json` (copy the example).
2. In a Yocto bbappend:
   ```bitbake
   # webconfig-schemas.bbappend
   WEBCONFIG_SCHEMAS_EXTRA:append = " file://my-app.schema.json"
   ```
3. The setting appears in the UI and is enforced server-side. No `webconfigd`
   rebuild, no frontend change.