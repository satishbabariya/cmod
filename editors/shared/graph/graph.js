/**
 * cmod Module Graph Renderer
 *
 * Renders a force-directed graph from cmod graph --format json --status --timing output.
 * Shared between VS Code and CLion/JCEF extensions.
 *
 * Expected data format (keyed by module name):
 * {
 *   "module_name": {
 *     "id": "src/mod.cppm",
 *     "name": "module_name",
 *     "kind": "InterfaceUnit",
 *     "source": "src/mod.cppm",
 *     "package": "myproject",
 *     "imports": ["dep_module"],
 *     "partition_of": null,
 *     "status": "up-to-date",        // optional
 *     "build_time_ms": 150            // optional
 *   }
 * }
 */

/* global d3 */

var _simulation = null;
var _svg = null;
var _graphGroup = null;
var _zoom = null;

function renderGraph(data) {
  // Parse nodes and links from the graph data
  var nodes = [];
  var links = [];
  var nodeMap = {};

  Object.keys(data).forEach(function(key) {
    var entry = data[key];
    var node = {
      id: entry.name || key,
      name: entry.name || key,
      kind: entry.kind || 'InterfaceUnit',
      source: entry.source || entry.id || '',
      package: entry.package || '',
      imports: entry.imports || [],
      status: entry.status || 'never-built',
      buildTimeMs: entry.build_time_ms || null,
      partitionOf: entry.partition_of || null
    };
    nodes.push(node);
    nodeMap[node.id] = node;
  });

  // Create links from imports
  nodes.forEach(function(node) {
    node.imports.forEach(function(imp) {
      if (nodeMap[imp]) {
        links.push({ source: node.id, target: imp });
      }
    });
  });

  // Update stats
  var statsEl = document.getElementById('stats');
  var upToDate = nodes.filter(function(n) { return n.status === 'up-to-date'; }).length;
  statsEl.textContent = nodes.length + ' modules, ' + upToDate + '/' + nodes.length + ' up-to-date';

  // Clear previous
  var svgEl = document.getElementById('graph');
  svgEl.innerHTML = '';

  var width = window.innerWidth;
  var height = window.innerHeight;

  _svg = d3.select('#graph')
    .attr('width', width)
    .attr('height', height);

  // Arrow marker
  _svg.append('defs').append('marker')
    .attr('id', 'arrowhead')
    .attr('viewBox', '0 -5 10 10')
    .attr('refX', 20)
    .attr('refY', 0)
    .attr('markerWidth', 6)
    .attr('markerHeight', 6)
    .attr('orient', 'auto')
    .append('path')
    .attr('d', 'M0,-5L10,0L0,5')
    .attr('fill', '#555');

  _zoom = d3.zoom()
    .scaleExtent([0.1, 4])
    .on('zoom', function(event) {
      _graphGroup.attr('transform', event.transform);
    });

  _svg.call(_zoom);
  _graphGroup = _svg.append('g');

  // Create tooltip
  var tooltip = d3.select('body').append('div')
    .attr('class', 'tooltip')
    .style('display', 'none');

  // Force simulation
  _simulation = d3.forceSimulation(nodes)
    .force('link', d3.forceLink(links).id(function(d) { return d.id; }).distance(100))
    .force('charge', d3.forceManyBody().strength(-300))
    .force('center', d3.forceCenter(width / 2, height / 2))
    .force('collision', d3.forceCollide().radius(50));

  // Links
  var linkGroup = _graphGroup.append('g').selectAll('line')
    .data(links)
    .enter().append('line')
    .attr('class', 'link');

  // Nodes
  var nodeGroup = _graphGroup.append('g').selectAll('g')
    .data(nodes)
    .enter().append('g')
    .attr('class', function(d) {
      var cls = 'node';
      cls += ' ' + d.status.replace(/\s+/g, '-');
      if (d.buildTimeMs !== null) {
        if (d.buildTimeMs < 100) cls += ' timing-fast';
        else if (d.buildTimeMs < 500) cls += ' timing-moderate';
        else cls += ' timing-slow';
      } else {
        cls += ' timing-none';
      }
      return cls;
    })
    .call(d3.drag()
      .on('start', dragstarted)
      .on('drag', dragged)
      .on('end', dragended));

  // Node rectangles
  nodeGroup.append('rect')
    .attr('width', function(d) { return Math.max(80, d.name.length * 8 + 20); })
    .attr('height', 32)
    .attr('x', function(d) { return -Math.max(80, d.name.length * 8 + 20) / 2; })
    .attr('y', -16);

  // Node labels
  nodeGroup.append('text')
    .attr('dy', function(d) { return d.buildTimeMs !== null ? -3 : 0; })
    .text(function(d) { return d.name; });

  // Timing labels
  nodeGroup.filter(function(d) { return d.buildTimeMs !== null; })
    .append('text')
    .attr('class', 'timing-label')
    .attr('dy', 10)
    .text(function(d) { return d.buildTimeMs + 'ms'; });

  // Click handler: open source file
  nodeGroup.on('click', function(event, d) {
    if (window._postMessage) {
      window._postMessage({ type: 'openFile', path: d.source });
    }
  });

  // Hover handlers
  nodeGroup
    .on('mouseover', function(event, d) {
      var info = '<strong>' + d.name + '</strong><br/>' +
        'Kind: ' + d.kind + '<br/>' +
        'Source: ' + d.source + '<br/>' +
        'Status: ' + d.status;
      if (d.buildTimeMs !== null) info += '<br/>Build: ' + d.buildTimeMs + 'ms';
      if (d.partitionOf) info += '<br/>Partition of: ' + d.partitionOf;
      info += '<br/>Imports: ' + (d.imports.length > 0 ? d.imports.join(', ') : 'none');
      tooltip.html(info)
        .style('display', 'block')
        .style('left', (event.pageX + 12) + 'px')
        .style('top', (event.pageY - 10) + 'px');

      // Highlight connected links
      linkGroup.classed('highlighted', function(l) {
        return l.source.id === d.id || l.target.id === d.id;
      });
    })
    .on('mousemove', function(event) {
      tooltip.style('left', (event.pageX + 12) + 'px')
        .style('top', (event.pageY - 10) + 'px');
    })
    .on('mouseout', function() {
      tooltip.style('display', 'none');
      linkGroup.classed('highlighted', false);
    });

  // Simulation tick
  _simulation.on('tick', function() {
    linkGroup
      .attr('x1', function(d) { return d.source.x; })
      .attr('y1', function(d) { return d.source.y; })
      .attr('x2', function(d) { return d.target.x; })
      .attr('y2', function(d) { return d.target.y; });

    nodeGroup.attr('transform', function(d) { return 'translate(' + d.x + ',' + d.y + ')'; });
  });

  // Filter input
  var filterInput = document.getElementById('filter');
  filterInput.addEventListener('input', function() {
    var query = this.value.toLowerCase();
    if (!query) {
      nodeGroup.classed('dimmed', false);
      linkGroup.classed('dimmed', false);
      return;
    }
    nodeGroup.classed('dimmed', function(d) {
      return d.name.toLowerCase().indexOf(query) === -1;
    });
    linkGroup.classed('dimmed', function(d) {
      var srcMatch = d.source.id.toLowerCase().indexOf(query) !== -1;
      var tgtMatch = d.target.id.toLowerCase().indexOf(query) !== -1;
      return !srcMatch && !tgtMatch;
    });
  });

  // Reset zoom button
  document.getElementById('resetZoom').addEventListener('click', function() {
    _svg.transition().duration(500).call(
      _zoom.transform, d3.zoomIdentity.translate(width / 2, height / 2).scale(0.8).translate(-width / 2, -height / 2)
    );
  });

  // Handle window resize
  window.addEventListener('resize', function() {
    var w = window.innerWidth;
    var h = window.innerHeight;
    _svg.attr('width', w).attr('height', h);
    _simulation.force('center', d3.forceCenter(w / 2, h / 2));
    _simulation.alpha(0.3).restart();
  });
}

function dragstarted(event, d) {
  if (!event.active) _simulation.alphaTarget(0.3).restart();
  d.fx = d.x;
  d.fy = d.y;
}

function dragged(event, d) {
  d.fx = event.x;
  d.fy = event.y;
}

function dragended(event, d) {
  if (!event.active) _simulation.alphaTarget(0);
  d.fx = null;
  d.fy = null;
}
