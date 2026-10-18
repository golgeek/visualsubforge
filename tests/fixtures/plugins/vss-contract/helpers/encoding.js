// Independently authored UTF-16 observation encoding; avoids HTML escaping.
// eslint-disable-next-line @typescript-eslint/no-unused-vars -- Loaded into shared script globals.
function contractHex(value) {
  var result = '';
  for (var index = 0; index < value.length; index += 1) {
    result += ('0000' + value.charCodeAt(index).toString(16)).slice(-4);
  }
  return result;
}
