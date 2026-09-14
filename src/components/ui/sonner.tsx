import { Toaster as SonnerToaster } from "sonner";

function Toaster() {
  return (
    <SonnerToaster
      theme="dark"
      position="top-right"
      richColors
      closeButton
      toastOptions={{
        style: {
          background: "#0e1118",
          border: "1px solid rgb(255 255 255 / 0.1)",
          color: "#e6e9f0",
          fontFamily: "'Segoe UI', 'Microsoft YaHei', sans-serif",
          fontSize: 13,
        },
      }}
    />
  );
}

export { Toaster };
