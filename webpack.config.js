const CopyWebpackPlugin = require("copy-webpack-plugin");
const path = require('path');
const webpack = require('webpack');

module.exports = {
  entry: "./bootstrap.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "bootstrap.js",
  },
  mode: "development",
  resolve: {
      // Use our versions of Node modules.
      alias: {
          'fs': 'browserfs/dist/shims/fs.js',
          'buffer': 'browserfs/dist/shims/buffer.js',
          'path': 'browserfs/dist/shims/path.js',
          'processGlobal': 'browserfs/dist/shims/process.js',
          'bufferGlobal': 'browserfs/dist/shims/bufferGlobal.js',
          'bfsGlobal': require.resolve('browserfs'),
          'perf_hooks': path.resolve(__dirname, 'stubs/perf_hooks.js'),
      }
  },
  plugins: [
      // Expose BrowserFS, process, and Buffer globals.
      // NOTE: If you intend to use BrowserFS in a script tag, you do not need
      // to expose a BrowserFS global.
      new webpack.ProvidePlugin({ BrowserFS: 'bfsGlobal', process: 'processGlobal', Buffer: 'bufferGlobal' }),
      new CopyWebpackPlugin(['index.html'])
    ],
    // DISABLE Webpack's built-in process and Buffer polyfills!
    node: {
        process: false,
        Buffer: false
    },
    module: {
      noParse: /browserfs\.js/,
      rules: [{
          test: /\.elm$/,
          exclude: [/elm-stuff/, /node_modules/],
          use: {
              loader: 'elm-webpack-loader',
              options: {}
          }
      }]
  }
};
