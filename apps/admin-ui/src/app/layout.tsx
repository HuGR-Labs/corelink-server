import * as React from "react";

export const metadata = {
  title: "CoreLink Admin",
  description: "CoreLink tenant administration console.",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
