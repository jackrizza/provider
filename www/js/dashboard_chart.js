/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
const ctx = document.getElementById("landing_chart");

new Chart(ctx, {
  type: "line",
  data: {
    labels: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul"],
    datasets: [
      {
        label: "Dataset 1",
        data: [12, 19, 3, 5, 2, 3, 9],
        borderWidth: 2,
        borderColor: "rgba(54, 162, 235, 1)",
        backgroundColor: "rgba(54, 162, 235, 0.2)",
        tension: 0.25, // optional: small smoothing
        pointRadius: 0, // optional: hide points for a “bare” look
      },
    ],
  },
  options: {
    responsive: true,
    plugins: { legend: { display: false }, title: { display: false } },
    scales: {
      x: { display: true, grid: { display: false } },
      y: { display: true, grid: { display: false } },
    },
  },
});
