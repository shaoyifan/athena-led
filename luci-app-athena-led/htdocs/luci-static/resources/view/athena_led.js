'use strict';
'require view';
'require form';
'require poll';
'require uci';
'require fs';
'require rpc';

var callServiceList = rpc.declare({
	object: 'service',
	method: 'list',
	params: ['name'],
	expect: { "": {} }
});

return view.extend({
	load: function () {
		return Promise.all([
			L.resolveDefault(fs.stat('/usr/sbin/athena-led'), null),
			uci.load('athena_led')
		]);
	},

	render: function (data) {

		// Check binary existence
		var fileExists = data[0];
		if (!fileExists) {
			return E('div', { 'class': 'alert-message error' },
				_('The athena-led binary was not found. Please download the file.'));
		}

		// Render Configuration Page
		var m, s, o;

		m = new form.Map('athena_led', _('Athena LED Ctrl'),
			_('JDCloud AX6600 LED Screen Ctrl'));


		// Status
		s = m.section(form.NamedSection, '__status__');
		s.anonymous = true;
		s.render = L.bind(function () {
			var statusEl = E('span', _('Collecting data...'));
			var container = E('div', { 'class': 'cbi-section' },
				E('p', { 'data-package': 'athena_led' }, statusEl)
			);

			callServiceList('athena_led').then(function (res) {
				var instances = (res && res.athena_led && res.athena_led.instances) || {};
				var isRunning = Object.values(instances).some(function (inst) {
					return inst.running === true;
				});

				statusEl.style.color = isRunning ? 'green' : 'red';
				statusEl.style.fontWeight = 'bold';
				statusEl.textContent = (isRunning ? '✅ ' : '❌ ') + (isRunning ? _('LED RUNNING') : _('LED NOT RUNNING'));
			}).catch(function (err) {
				statusEl.style.color = 'red';
				statusEl.textContent = '⚠️ ' + _('Error loading status');
				console.error(err);
			});

			return container;
		}, s);

		// Settings
		s = m.section(form.TypedSection, 'athena_led');
		s.anonymous = true;
		s.addremove = false;

		o = s.option(form.Flag, 'Enabled', _('Enabled'));
		o.default = '0';
		o.rmempty = false;

		o = s.option(form.ListValue, 'IntervalTime', _('LED Interval Time'),
			_('Set the refresh interval of the LED screen'));
		o.default = '5';
		o.rmempty = false;
		o.value('1', '1 ' + _('Seconds'));
		o.value('2', '2 ' + _('Seconds'));
		o.value('3', '3 ' + _('Seconds'));
		o.value('4', '4 ' + _('Seconds'));
		o.value('5', '5 ' + _('Seconds'));

		o = s.option(form.ListValue, 'LightLevel', _('LED Light Level'),
			_('Set the brightness level of the LED screen'));
		o.default = '5';
		o.rmempty = false;
		o.value('0', _('Brightness Level') + ' 0');
		o.value('1', _('Brightness Level') + ' 1');
		o.value('2', _('Brightness Level') + ' 2');
		o.value('3', _('Brightness Level') + ' 3');
		o.value('4', _('Brightness Level') + ' 4');
		o.value('5', _('Brightness Level') + ' 5');
		o.value('6', _('Brightness Level') + ' 6');
		o.value('7', _('Brightness Level') + ' 7');

		o = s.option(form.MultiValue, 'SideStatusIndicator', _('Side Status Indicator'),
			_('Set the display mode of the side status indicator light'));
		o.default = '';
		o.rmempty = true;
		o.value('clock', _('Clock Indicator Light'));
		o.value('medal', _('Medal Indicator Light'));
		o.value('upload', _('Upload Indicator Light'));
		o.value('download', _('Download Indicator Light'));

		o = s.option(form.MultiValue, 'DisplayMode', _('LED Display Mode'),
			_('Set the display mode of the LED screen'));
		o.default = 'date timeBlink';
		o.rmempty = false;
		o.value('date', _('Current Date'));
		o.value('time', _('Current Time'));
		o.value('timeBlink', _('Current Time (Blinking)'));
		o.value('temp', _('Device Temperature'));
		o.value('string', _('Custom Text'));
		o.value('getByUrl', _('Remote Text'));

		o = s.option(form.Value, 'CustomText', _('Custom Text'),
			_('Set the content displayed in \'Custom Text\' mode (allowed characters: [a~z], [0~9], [+-*/=.:：℃] )')
		);
		o.default = 'abcdefghijklmnopqrstuvwxyz0123456789+-*/=.:：℃';
		o.rmempty = false;
		o.placeholder = _('Enter your message here');

		o = s.option(form.Value, 'RemoteText', _('Remote Text'),
			_('Set the URL for retrieving text in \'Remote Text\' mode'));
		o.default = 'https://ifconfig.me';
		o.rmempty = false;
		o.placeholder = _('Enter your api url here');

		o = s.option(form.MultiValue, 'TemperatureSensor', _('Temperature Sensor'),
			_('Select the sensor displayed in the \'Device Temperature\' mode'));
		o.default = '4';
		o.rmempty = false;
		o.value('0', _('NSS-TOP'));
		o.value('1', _('NSS'));
		o.value('2', _('WCSS-PHYA0'));
		o.value('3', _('WCSS-PHYA1'));
		o.value('4', _('CPU'));
		o.value('5', _('LPASS'));
		o.value('6', _('DDESS-TOP'));

		return m.render();
	}
});