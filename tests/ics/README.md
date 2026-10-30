## Test Files

There are several structural metadata files in this directory that can be used
to test a dCDN's implementation of the CDNI specifications. Each of the files is
designed to test certain parts of the CDNI specifications, as outlined below.

The files are designed to _emulate_ a real-world deployment where the ICS that
holds these values is supporting a dCDN. In some cases, the `Source` Generic
Metadata information is used to specify the uCDN's IP address where original
content is found. In others, synthetic responses are generating according to
Processing Stages Generic Metadata.

A real deployment's actual structural metadata may not look anything like the
files here -- these files are for testing purposes, mainly.

### `cnn-https-source.json`

This CDNI structural metadata file uses metadata objects from the Processing
Stages draft in order to specify that the request be rewritten to serve content
from CNN.com fetched over HTTPS.

Use
[`put-example-to-cnn-https-source.curl`](./put-example-to-cnn-https-source.curl)
to add this metadata to the ICS running locally and associate it with the
`example.com` host.

On a dCDN that handles a request for `example.com` and uses the structural
metadata from `cnn-https-source.json` to guide its generation of a response, the
contents of CNN.com should be returned.

### `example-client-origin-request-response-ordering.json`

This CDNI structural metadata file uses metadata objects from the Processing
Stages draft in order to specify that the request be (ultimately) rewritten to
serve content from example.com and add/remove headers in a way that test whether
the dCDN interprets the Processing Stages metadata stages in the proper order.

**Client Request Stage**

Transforms the request by

- Deleting the client request's `HOST` header.
- Adding
  - A new `HOST` header with the value `www.altavista.com`
  - A new `client-request-request-xform-new-later-removed` header whose value is
    the request's URI path (as computed by a MEL expression)
  - A new `client-request-request-xform-new` header whose value is the request's
    URI path (as computed by a MEL expression)

Transforms the response by

- Deleting
  - the response's `transfer-encoding` header.
  - the response's `origin-request-response-xform-new-later-removed`

**Origin Request Stage**

Transforms the request by

- Deleting
  - the request's `HOST` header
  - the request's `client-request-request-xform-new-later-removed`
- Adding
  - A new `HOST` header with the value `www.example.com`
  - A new `origin-request-request-xform-new` header whose value is the request's
    URI path (as computed by a MEL expression)

Transforms the response by

- Deleting
  - the response's `transfer-encoding` header.
- Adding
  - a new `origin-request-response-xform-new-later-removed` header whose value
    is the request's URI path (as computed by a MEL expression)
  - a new `origin-request-response-xform-new` header whose value is the
    request's URI path (as computed by a MEL expression)

Use
[`./put-linkedin-to-example-client-origin-request-response-ordering.curl`](./put-linkedin-to-example-client-origin-request-response-ordering.curl)
to add this metadata to the ICS running locally and associate it with the
`linkedin.com` host.

On a dCDN that handles a request for `linkedin.com` and uses the structural
metadata from `example-client-origin-request-response-ordering.json` to guide
its generation of a response, the request to the origin should

- have `www.example.com` as the value of the `HOST` header
- have a header named `client-request-request-xform-new`
- have a header named `origin-request-request-xform-new`

and response to the client should

- have a header named `client-request-response-xform-new`
- have a header named `origin-request-response-xform-new`

The body of the response should contain the contents of `example.com`.

### `gmail-synthetic-source.json`

This CDNI structural metadata file uses metadata objects from the Processing
Stages draft in order to specify that the response to the request be
synthetically generated from the result of evaluating a MEL expression.

The body of the response should contain the result of evaluating the MEL
expression with the source code

```
"using builtin mel string functions: " . upper("one"),
```

(i.e., `using builtin mel string functions: ONE`)

and should have a status code of 200.

Use [`./put-gmail-synthetic.curl`](./put-gmail-synthetic.curl) to add this
metadata to the ICS running locally and associate it with the `gmail.com` host.

### `yahoo-synthetic-set-variable.json`

This CDNI structural metadata file uses metadata objects from the Processing
Stages draft in order to specify that the response to the request be
synthetically generated from the result of evaluating a MEL expression that uses
a variable whose value is set with the `MI.SetVariable` generic metadata object.

The variable `testing` (whose value is `1`) is available to MEL expressions when
processing the structural metadata in this file thanks to the `MI.SetVariable`
generic metadata object it contains.

The body of the response should contain the result of evaluating the MEL
expression with the source code

```
"Value of variable set using MI.SetVariable in HMD: " . string(var^testing)
```

(i.e., `Value of variable set using MI.SetVariable in HMD: 1`)

and should have a status code of 200.

Use
[`./put-yahoo-synthetic-set-variable.curl`](./put-yahoo-synthetic-set-variable.curl)
to add this metadata to the ICS running locally and associate it with the
`yahoo.com` host.

### `example-cors-allow-origin.json`

This CDNI structural metadata file uses metadata objects from the Edge Control
Metadata draft in order to specify CORS access-control headers to be added to a
response. The `MI.CrossoriginPolicy` specifies that the
Access-Control-Allow-Origin header is added to the response (with a wildcard
value) when the client request's `ORIGIN` header field contains the value
`http://sourcepage.example.com`.

The Access-Control-Allow-Origin header field should be set to `*` when a dCDN
responds to a request from the origin `http://sourcepage.example.com` (as
indicated by the presence of the `ORIGIN` header with the proper value) for
content from a host associated with the structural metadata in this file.

Use
[`./put-lycos-to-example-cors-allow-origin.curl`](./put-lycos-to-example-cors-allow-origin.curl)
to add this metadata to the ICS running locally and associate it with the
`lycos.com` host.
