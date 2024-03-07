const basePath = process.env['BASE_PATH'];

/** @type {import('next').NextConfig} */
const nextConfig = {
  output: 'export',
  basePath: basePath ?? undefined,
  assetPrefix: basePath ? basePath + '/' : undefined
};

export default nextConfig;
