"use client";

import * as React from "react";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/Button";
import { Table, type TableColumn } from "@/components/ui/Table";
import { formatDate } from "@/i18n/format";
import type { Locale } from "@/i18n/LocaleContext";
import type { SubProcessor } from "@/content/load";

const TITLE: Record<Locale, string> = {
  en: "Sub-processors",
  pt: "Subprocessadores",
  es: "Subprocesadores",
};

const HEADERS: Record<Locale, { name: string; role: string; region: string; certs: string; audit: string; download: string }> = {
  en: {
    name: "Name",
    role: "Role",
    region: "Region",
    certs: "Certifications",
    audit: "Last audit",
    download: "Download CSV",
  },
  pt: {
    name: "Nome",
    role: "Função",
    region: "Região",
    certs: "Certificações",
    audit: "Última auditoria",
    download: "Baixar CSV",
  },
  es: {
    name: "Nombre",
    role: "Función",
    region: "Región",
    certs: "Certificaciones",
    audit: "Última auditoría",
    download: "Descargar CSV",
  },
};

export interface SubProcessorsPageProps {
  locale: Locale;
  version: string;
  items: SubProcessor[];
}

function toCsv(items: SubProcessor[]): string {
  const header = ["id", "name", "role", "region", "certifications", "last_audit"];
  const rows = items.map((it) =>
    [it.id, it.name, it.role, it.region, it.certifications.join("|"), it.last_audit]
      .map((v) => `"${String(v).replace(/"/g, '""')}"`)
      .join(",")
  );
  return [header.join(","), ...rows].join("\n");
}

export function SubProcessorsPage({ locale, version, items }: SubProcessorsPageProps) {
  const headers = HEADERS[locale];

  const columns: TableColumn<SubProcessor>[] = [
    { key: "name", header: headers.name, sortable: true },
    { key: "role", header: headers.role },
    { key: "region", header: headers.region, sortable: true },
    {
      key: "certifications",
      header: headers.certs,
      render: (row) => row.certifications.join(", "),
    },
    {
      key: "last_audit",
      header: headers.audit,
      sortable: true,
      render: (row) => formatDate(row.last_audit, locale),
    },
  ];

  function handleDownload() {
    const csv = toCsv(items);
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `sub-processors-${version}.csv`;
    a.rel = "noopener";
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  return (
    <article className="mx-auto max-w-4xl py-8">
      <PageHeader
        title={TITLE[locale]}
        description={`v${version}`}
        actions={
          <Button variant="secondary" onClick={handleDownload}>
            {headers.download}
          </Button>
        }
      />
      <Table caption={TITLE[locale]} columns={columns} rows={items.map((i) => ({ ...i, id: i.id }))} />
    </article>
  );
}

export { toCsv };
