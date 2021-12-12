FROM node:lts-alpine as build-image

WORKDIR /root/app

COPY ./package.json ./
COPY ./tsconfig.json ./
COPY ./src ./src

RUN npm i && npm run build


FROM node:lts-alpine as runtime-image

WORKDIR /root/app

COPY ./package.json ./

RUN npm i --only=production

COPY --from=build-image /root/app/build ./build

CMD npm run run
