import type { ReactNode } from 'react';

export function ApiEndpoint({ method, path }: { method: 'GET' | 'POST'; path: string }) {
  return (
    <div className="api-endpoint not-prose">
      <div className="api-endpoint-route">
        <span className={`api-method api-method-${method.toLowerCase()}`}>{method}</span>
        <span className="api-path">{path}</span>
      </div>
      <span className="api-endpoint-meta">Bearer Token · JSON</span>
    </div>
  );
}

export function ApiParameters({ children }: { children: ReactNode }) {
  return <dl className="api-parameters not-prose">{children}</dl>;
}

export function ApiParameter({ name, type, required, defaultValue, children }: {
  name: string;
  type: string;
  required?: string;
  defaultValue?: string;
  children: ReactNode;
}) {
  return (
    <div className="api-parameter">
      <dt>
        <code>{name}</code>
        <span className="api-parameter-type">{type}</span>
        {required && <span className="api-parameter-required">{required}</span>}
        {defaultValue && <span className="api-parameter-default">{defaultValue}</span>}
      </dt>
      <dd>{children}</dd>
    </div>
  );
}
