'use strict';

const form = document.getElementById('login-form');
const errorBox = document.getElementById('login-error');

form.addEventListener('submit', async (event) => {
  event.preventDefault();
  errorBox.hidden = true;
  const button = form.querySelector('button');
  button.disabled = true;
  try {
    const response = await fetch('/api/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        user: document.getElementById('login-user').value.trim(),
        password: document.getElementById('login-password').value,
      }),
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || 'Accesso non riuscito');
    location.reload();
  } catch (error) {
    errorBox.textContent = error.message;
    errorBox.hidden = false;
  } finally {
    button.disabled = false;
  }
});
