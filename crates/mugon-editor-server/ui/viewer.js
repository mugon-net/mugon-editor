import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

export class Viewer {
  constructor(container) {
    this.container = container;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    this.renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    container.appendChild(this.renderer.domElement);
    this.scene = new THREE.Scene();
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x445066, 1.6));
    const sun = new THREE.DirectionalLight(0xffffff, 2.2);
    sun.position.set(3, 6, 4);
    this.scene.add(sun);
    this.camera = new THREE.PerspectiveCamera(40, 1, 0.01, 5000);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.clock = new THREE.Clock();
    this.loader = new GLTFLoader();
    this.textureLoader = new THREE.TextureLoader();
    this.model = null;
    this.mixer = null;
    this.clips = [];
    this.wire = false;
    this.token = 0;
    new ResizeObserver(() => this.resize()).observe(container);
    this.renderer.setAnimationLoop(() => {
      if (this.mixer) this.mixer.update(this.clock.getDelta());
      this.controls.update();
      this.renderer.render(this.scene, this.camera);
    });
  }

  resize() {
    const w = this.container.clientWidth, h = this.container.clientHeight;
    if (!w || !h) return;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  clear() {
    if (this.model) this.scene.remove(this.model);
    if (this.boxHelper) { this.scene.remove(this.boxHelper); this.boxHelper = null; }
    this.model = null; this.mixer = null; this.clips = [];
  }

  async load(url) {
    const token = ++this.token;
    this.clear();
    const gltf = await this.loader.loadAsync(url);
    if (token !== this.token) return null;
    this.model = gltf.scene;
    this.scene.add(this.model);
    this.clips = gltf.animations;
    if (this.clips.length) this.mixer = new THREE.AnimationMixer(this.model);
    this.setWireframe(this.wire);
    this.frame();
    return gltf;
  }

  frame() {
    const box = new THREE.Box3().setFromObject(this.model);
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    const radius = Math.max(size.length() / 2, 0.001);
    const dist = radius / Math.sin((this.camera.fov * Math.PI) / 360);
    this.camera.near = radius / 100;
    this.camera.far = radius * 100;
    this.camera.position.copy(center).add(new THREE.Vector3(0.8, 0.55, 1).normalize().multiplyScalar(dist));
    this.camera.updateProjectionMatrix();
    this.controls.target.copy(center);
    this.controls.update();
  }

  setWireframe(on) {
    this.wire = on;
    this.model?.traverse((o) => {
      if (o.isMesh) (Array.isArray(o.material) ? o.material : [o.material]).forEach((m) => (m.wireframe = on));
    });
  }

  setBounds(on) {
    if (this.boxHelper) { this.scene.remove(this.boxHelper); this.boxHelper = null; }
    if (on && this.model) {
      this.boxHelper = new THREE.Box3Helper(new THREE.Box3().setFromObject(this.model), 0xffd24a);
      this.scene.add(this.boxHelper);
    }
  }

  async setTexture(url) {
    if (!this.model) return;
    const tex = await this.textureLoader.loadAsync(url);
    tex.flipY = false;
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.magFilter = THREE.NearestFilter;
    tex.minFilter = THREE.NearestFilter;
    this.model.traverse((o) => {
      if (o.isMesh) (Array.isArray(o.material) ? o.material : [o.material]).forEach((m) => { m.map = tex; m.needsUpdate = true; });
    });
  }

  play(index) {
    if (!this.mixer) return;
    this.mixer.stopAllAction();
    if (index >= 0) this.mixer.clipAction(this.clips[index]).play();
  }
}
