# ==========================================
# ESTÁGIO 1: BUILD DO VUEJS VIA VITE
# ==========================================
FROM node:20-slim as builder
WORKDIR /app
# Copy VERSION from repo root (needed by vite.config.ts)
COPY VERSION ../VERSION
COPY frontend/package*.json ./
RUN npm install
COPY frontend/ .
# Transpilação Javascript e CSS Modules
RUN npm run build

# ==========================================
# ESTÁGIO 2: NGINX ORCHESTRATOR
# ==========================================
FROM nginx:alpine

# Descartar conteúdo default HTML do Nginx
RUN rm -rf /usr/share/nginx/html/*

# Transportar o pacote SPA (dist) feito em TS/Vite pro web root
COPY --from=builder /app/dist /usr/share/nginx/html

# Importar a nossa rota máster do Proxy Nginx
COPY frontend/nginx.conf /etc/nginx/conf.d/default.conf

# Espor interface pra Web World Wide
EXPOSE 80

# Iniciar server
CMD ["nginx", "-g", "daemon off;"]
