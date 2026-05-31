// k6 load test for fenestra OGC server
// Usage: k6 run scripts/load_test.js
import http from "k6/http";
import { check, sleep } from "k6";

const BASE_URL = __ENV.BASE_URL || "http://localhost:8080";

export const options = {
  stages: [
    { duration: "10s", target: 10 },
    { duration: "30s", target: 50 },
    { duration: "10s", target: 0 },
  ],
  thresholds: {
    http_req_duration: ["p(95)<500"],
    http_req_failed: ["rate<0.01"],
  },
};

export default function () {
  // Health check
  const health = http.get(`${BASE_URL}/health`);
  check(health, { "health 200": (r) => r.status === 200 });

  // WMS GetCapabilities
  const wmsCap = http.get(`${BASE_URL}/wms?service=WMS&request=GetCapabilities`);
  check(wmsCap, { "wms capabilities 200": (r) => r.status === 200 });

  // WMS GetMap
  const wmsMap = http.get(
    `${BASE_URL}/wms?service=WMS&request=GetMap&layers=default&styles=&crs=EPSG:4326&bbox=-1,50,1,52&width=256&height=256&format=image/png`
  );
  check(wmsMap, { "wms getmap 200": (r) => r.status === 200 });

  // WFS GetCapabilities
  const wfsCap = http.get(`${BASE_URL}/wfs?request=GetCapabilities`);
  check(wfsCap, { "wfs capabilities 200": (r) => r.status === 200 });

  // WFS GetFeature
  const wfsFeature = http.get(
    `${BASE_URL}/wfs?request=GetFeature&type_names=buildings&count=10&bbox=-1,50,1,52`
  );
  check(wfsFeature, { "wfs getfeature 200": (r) => r.status === 200 });

  sleep(0.1);
}
