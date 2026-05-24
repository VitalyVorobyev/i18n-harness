/// <reference types="vite/client" />

declare module "*.module.css" {
  const classes: { readonly [name: string]: string };
  export default classes;
}
